// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use core::convert::TryInto;
use core::ops::{Deref, DerefMut};

use crate::{Phy, PhyRw, Trace};
use ringbuf::ringbuf_entry_root as ringbuf_entry;
use userlib::hl::sleep_for;
use vsc7448_pac::phy;
use vsc_err::VscError;

pub struct Vsc8562Phy<'a, 'b, P>(pub &'b mut Phy<'a, P>);
impl<'a, 'b, P> Deref for Vsc8562Phy<'a, 'b, P> {
    type Target = Phy<'a, P>;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}
impl<'a, 'b, P> DerefMut for Vsc8562Phy<'a, 'b, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, 'b, P: PhyRw> Vsc8562Phy<'a, 'b, P> {
    /// Initializes a VSC8562 PHY using SGMII based on section 3.1.2.1 (2x SGMII
    /// to 100BASE-FX SFP Fiber).  Same caveats as `init` apply.
    pub fn init(&mut self) -> Result<(), VscError> {
        // This is roughly based on `vtss_phy_reset_private`
        ringbuf_entry!(Trace::Vsc8562Init(self.port));
        self.check_base_port()?;

        // Apply the initial patch (more patches to SerDes happen later)
        crate::viper::ViperPhy(self.0).patch()?;

        self.broadcast(|v| {
            v.modify(phy::GPIO::MAC_MODE_AND_FAST_LINK(), |r| {
                // MAC configuration = SGMII
                r.0 &= !(0b11 << 14)
            })
        })?;

        // Enable 2 port MAC SGMII, then wait for the command to finish
        self.cmd(0x80F0)?;

        ////////////////////////////////////////////////////////////////////////
        if !self.sd6g_has_patch()? {
            self.sd6g_patch()?;
        }

        // 100BASE-FX on all PHYs
        self.cmd(0x8FD1)?;

        self.broadcast(|v| {
            v.modify(phy::STANDARD::EXTENDED_PHY_CONTROL(), |r| {
                // SGMII MAC interface mode
                r.set_mac_interface_mode(0);
                // 100BASE-FX fiber/SFP on the fiber media pins only
                r.set_media_operating_mode(0b11);
            })
        })?;

        // Now, we reset the PHY to put those settings into effect.  For some
        // reason, we can't do a broadcast reset, so we do it port-by-port.
        for p in 0..2 {
            Phy::new(self.port + p, self.rw).software_reset()?;
        }

        ////////////////////////////////////////////////////////////////////////
        // "Bug# 19146
        //  Adjust the 1G SerDes SigDet Input Threshold and Signal Sensitivity for 100FX"
        // Based on `vtss_phy_sd1g_patch_private` in the SDK
        for p in 0..2 {
            // XXX The SDK just does self.port * 2 (including any offset); based on
            // table 33 in the datasheet, I believe this is actually the correct
            // behavior.
            let slave_addr = p * 2;

            // "read 1G MCB into CSRs"
            self.mcb_read(0x20, slave_addr)?;

            // Various bits of configuration for 100FX mode
            self.sd1g_ib_cfg_write(0)?;
            self.sd1g_misc_cfg_write(1)?;
            self.sd1g_des_cfg_write(14, 3)?;

            // "write back 1G MCB"
            self.mcb_write(0x20, slave_addr)?;
        }

        ////////////////////////////////////////////////////////////////////////
        // "Fix for bz# 21484 ,TR.LinkDetectCtrl = 3"
        self.broadcast(|v| {
            v.write(phy::TR::TR_16(), 0xa7f8.into())?;
            v.modify(phy::TR::TR_17(), |r| {
                r.0 &= 0xffe7;
                r.0 |= 3 << 3;
            })?;
            v.write(phy::TR::TR_16(), 0x87f8.into())?;

            // "Fix for bz# 21485 ,VgaThresh100=25"
            v.write(phy::TR::TR_16(), 0xafa4.into())?;
            v.modify(phy::TR::TR_18(), |r| {
                r.0 &= 0xff80;
                r.0 |= 25
            })?;
            v.write(phy::TR::TR_16(), 0x8fa4.into())
        })?;

        // In the SDK, there's more configuration for 100BT, which we don't use

        Ok(())
    }

    /// `vtss_phy_chk_serdes_patch_init_private`
    fn sd6g_has_patch(&mut self) -> Result<bool, VscError> {
        self.mcb_read(0x3f, 0)?;
        let cfg0 = self.macsec_csr_read(7, 0x22)?;
        let ib_reg_pat_sel_offset = (cfg0 & 0x00000300) >> 8;

        // Hardware default is 1; we set it to 0
        if ib_reg_pat_sel_offset != 0 {
            return Ok(false);
        }

        let cfg2 = self.macsec_csr_read(7, 0x24)?;
        let ib_tcalv = (cfg2 & 0x000003e0) >> 5;
        let ib_ureg = (&0x00000007) >> 0;

        // Hardware default is ib_tcalv = 12, ib_ureg = 4
        if ib_tcalv != 13 || ib_ureg != 5 {
            return Ok(false);
        }

        let des = self.macsec_csr_read(7, 0x21)?;
        let des_bw_ana = (des & 0x0000000e) >> 1; // bit   3:1

        // This is configured for SGMII specifically
        Ok(des_bw_ana == 3)
    }

    /// Based on `vtss_phy_sd6g_patch_private`.
    ///
    /// `v` must be the base port of this PHY, otherwise this will return an error
    fn sd6g_patch(&mut self) -> Result<(), VscError> {
        self.check_base_port()?;

        let ib_sig_det_clk_sel_cal = 0; // "0 for during IBCAL for all"
        let ib_sig_det_clk_sel_mm = 7;
        let ib_tsdet_cal = 16;
        let ib_tsdet_mm = 5;

        let pll_fsm_ctrl_data = 60;
        let qrate = 1;
        let if_mode = 1;
        let des_bw_ana_val = 3;

        // `detune_pll5g`
        self.macsec_csr_modify(7, 0x8, |r| {
            *r &= 0xfffffc1e;
            *r |= 1; // ena_gain_test
        })?;

        // "0. Reset RCPLL"
        // "pll_fsm_ena=0, reset rcpll"
        self.sd6g_pll_cfg_write(3, pll_fsm_ctrl_data, 0)?;
        self.sd6g_common_cfg_write(0, 0, 0, qrate, if_mode, 0)?;
        self.mcb_write(0x3f, 0)?;

        // "1. Configure sd6g for SGMII prior to sd6g_IB_CAL"
        // "update des_bw_ana for bug 14948"
        let ib_rtrm_adj = 16 - 3;
        self.sd6g_des_cfg_write(6, 2, 5, des_bw_ana_val, 0)?;
        self.sd6g_ib_cfg0_write(ib_rtrm_adj, ib_sig_det_clk_sel_mm, 0, 0)?;
        self.sd6g_ib_cfg1_write(8, ib_tsdet_mm, 15, 0, 1)?;

        // "update ib_tcalv & ib_ureg for bug 14626"
        self.sd6g_ib_cfg2_write(3, 13, 5)?;
        self.sd6g_ib_cfg3_write(0, 31, 1, 31)?;
        self.sd6g_ib_cfg4_write(63, 63, 2, 63)?;

        self.sd6g_common_cfg_write(1, 1, 0, qrate, if_mode, 0)?; // "sys_rst, ena_lane"
        self.sd6g_misc_cfg_write(1)?; // "assert lane reset"
        self.mcb_write(0x3f, 0)?;

        // "2. Start rcpll_fsm"
        self.sd6g_pll_cfg_write(3, pll_fsm_ctrl_data, 1)?;
        self.mcb_write(0x3f, 0)?;

        // "3. Wait for PLL cal to complete"
        let mut timed_out = true;
        for _ in 0..300 {
            self.mcb_read(0x3f, 0)?;
            let rd_dat = self.macsec_csr_read(7, 0x31)?;
            // "wait for bit 12 to clear"
            if (rd_dat & 0x0001000) == 0 {
                timed_out = false;
                break;
            }
            sleep_for(1);
        }
        if timed_out {
            return Err(VscError::PhyPllCalTimeout);
        }

        // "4. Release digital reset and disable transmitter"
        self.sd6g_misc_cfg_write(0)?; // "release lane reset"
        self.sd6g_common_cfg_write(1, 1, 0, qrate, if_mode, 1)?; // "sys_rst, ena_lane, pwd_tx"
        self.mcb_write(0x3f, 0)?;

        // "5. Apply a frequency offset on RX-side (using internal FoJi logic)
        //  Make sure that equipment loop is not active. Already done above"
        self.sd6g_gp_cfg_write(768)?; // "release lane reset"
        self.sd6g_dft_cfg2_write(0, 2, 0, 0, 0, 1)?; // "release lane reset"
        self.sd6g_dft_cfg0_write(0, 0, 1)?;
        self.sd6g_des_cfg_write(6, 2, 5, des_bw_ana_val, 2)?;
        self.mcb_write(0x3f, 0)?;

        // "6. Prepare required settings for IBCAL"
        let gp_iter = 5;
        self.sd6g_ib_cfg1_write(8, ib_tsdet_cal, 15, 1, 0)?;
        self.sd6g_ib_cfg0_write(ib_rtrm_adj, ib_sig_det_clk_sel_cal, 0, 0)?;
        self.mcb_write(0x3f, 0)?;

        // "7. Start IB_CAL"
        self.sd6g_ib_cfg0_write(ib_rtrm_adj, ib_sig_det_clk_sel_cal, 0, 1)?;
        self.mcb_write(0x3f, 0)?;
        for _ in 0..gp_iter {
            self.sd6g_gp_cfg_write(769)?;
            self.mcb_write(0x3f, 0)?;
            self.sd6g_gp_cfg_write(768)?;
            self.mcb_write(0x3f, 0)?;
        }

        // "ib_filt_offset=1"
        self.sd6g_ib_cfg1_write(8, ib_tsdet_cal, 15, 1, 1)?;
        self.mcb_write(0x3f, 0)?;
        // "then ib_frc_offset=0"
        self.sd6g_ib_cfg1_write(8, ib_tsdet_cal, 15, 0, 1)?;
        self.mcb_write(0x3f, 0)?;

        // "8. Wait for IB cal to complete"
        let mut timed_out = true;
        for _ in 0..300 {
            self.mcb_read(0x3f, 0)?; // "read 6G MCB into CSRs"
            let rd_dat = self.macsec_csr_read(7, 0x2f)?; // "ib_status0"

            // "wait for bit 8 to set"
            if rd_dat & 0x0000100 != 0 {
                timed_out = false;
                break;
            }
            sleep_for(1);
        }
        if timed_out {
            return Err(VscError::PhyIbCalTimeout);
        }

        // "9. Restore cfg values for mission mode"
        self.sd6g_ib_cfg0_write(ib_rtrm_adj, ib_sig_det_clk_sel_mm, 0, 1)?;
        self.sd6g_ib_cfg1_write(8, ib_tsdet_mm, 15, 0, 1)?;
        self.mcb_write(0x3f, 0)?;

        // "10. Re-enable transmitter"
        self.sd6g_common_cfg_write(1, 1, 0, qrate, if_mode, 0)?;
        self.mcb_write(0x3f, 0)?;

        // "11. Disable frequency offset generation (using internal FoJi logic)"
        self.sd6g_dft_cfg2_write(0, 0, 0, 0, 0, 0)?;
        self.sd6g_dft_cfg0_write(0, 0, 0)?;
        self.sd6g_des_cfg_write(6, 2, 5, des_bw_ana_val, 0)?;
        self.mcb_write(0x3f, 0)?;

        // `tune_pll5g`
        // "Mask Off bit 0: ena_gain_test"
        self.macsec_csr_modify(7, 0x8, |r| *r &= 0xfffffffe)?;

        // "12. Configure for Final Configuration and Settings"
        // "a. Reset RCPLL"
        self.sd6g_pll_cfg_write(3, pll_fsm_ctrl_data, 0)?;
        self.sd6g_common_cfg_write(0, 1, 0, qrate, if_mode, 0)?;
        self.mcb_write(0x3f, 0)?;

        // "b. Configure sd6g for desired operating mode"
        // Settings for SGMII
        let pll_fsm_ctrl_data = 60;
        let qrate = 1;
        let if_mode = 1;
        let des_bw_ana_val = 3;
        self.cmd(0x80F0)?; // XXX: why do we need to do this again here?

        self.mcb_read(0x11, 0)?; // "read LCPLL MCB into CSRs"
        self.mcb_read(0x3f, 0)?; // "read 6G MCB into CSRs"

        // "update LCPLL bandgap voltage setting (bug 13887)"
        self.pll5g_cfg0_write(4)?;
        self.mcb_write(0x11, 0)?;

        // "update des_bw_ana for bug 14948"
        self.sd6g_des_cfg_write(6, 2, 5, des_bw_ana_val, 0)?;
        self.sd6g_ib_cfg0_write(ib_rtrm_adj, ib_sig_det_clk_sel_mm, 0, 1)?;
        self.sd6g_ib_cfg1_write(8, ib_tsdet_mm, 15, 0, 1)?;
        self.sd6g_common_cfg_write(1, 1, 0, qrate, if_mode, 0)?;

        // "update ib_tcalv & ib_ureg for bug 14626"
        self.sd6g_ib_cfg2_write(3, 13, 5)?;
        self.sd6g_ib_cfg3_write(0, 31, 1, 31)?;
        self.sd6g_ib_cfg4_write(63, 63, 2, 63)?;

        self.sd6g_misc_cfg_write(1)?;
        self.mcb_write(0x3f, 0)?;

        // "2. Start rcpll_fsm"
        self.sd6g_pll_cfg_write(3, pll_fsm_ctrl_data, 1)?;
        self.mcb_write(0x3f, 0)?;

        // "3. Wait for PLL cal to complete"
        let mut timed_out = true;
        for _ in 0..200 {
            self.mcb_read(0x3f, 0)?; // "read 6G MCB into CSRs"
            let rd_dat = self.macsec_csr_read(7, 0x31)?; // "pll_status"

            // "wait for bit 12 to clear"
            if rd_dat & 0x0001000 == 0 {
                timed_out = false;
                break;
            }
            sleep_for(1);
        }
        if timed_out {
            return Err(VscError::PhyPllCalTimeout);
        }

        self.sd6g_misc_cfg_write(0)?; // "release lane reset"
        self.mcb_write(0x3f, 0)?; // "write back 6G MCB"

        Ok(())
    }

    /// `vtss_phy_mcb_rd_trig_private`
    fn mcb_read(
        &mut self,
        mcb_reg_addr: u32,
        mcb_slave_num: u8,
    ) -> Result<(), VscError> {
        // Request a read from the MCB
        self.macsec_csr_write(
            7,
            mcb_reg_addr,
            0x40000000 | (1 << mcb_slave_num),
        )?;

        // Timeout based on the SDK SD6G_TIMEOUT
        for _ in 0..200 {
            let r = self.macsec_csr_read(7, mcb_reg_addr)?;
            if (r & 0x40000000) == 0 {
                return Ok(());
            }
            sleep_for(1);
        }
        Err(VscError::McbReadTimeout)
    }

    /// `vtss_phy_mcb_wr_trig_private`
    fn mcb_write(
        &mut self,
        mcb_reg_addr: u32,
        mcb_slave_num: u8,
    ) -> Result<(), VscError> {
        // Write back MCB
        self.macsec_csr_write(
            7,
            mcb_reg_addr,
            0x80000000 | (1 << mcb_slave_num),
        )?;

        // Timeout based on the SDK SD6G_TIMEOUT
        for _ in 0..200 {
            let r = self.macsec_csr_read(7, mcb_reg_addr)?;
            if (r & 0x80000000) == 0 {
                return Ok(());
            }
            sleep_for(1);
        }
        Err(VscError::McbWriteTimeout)
    }

    /// `vtss_phy_macsec_csr_rd_private`
    fn macsec_csr_read(
        &mut self,
        target: u16,
        csr_reg_addr: u32,
    ) -> Result<u32, VscError> {
        // "Wait for MACSEC register access"
        self.macsec_wait(19)?;

        // "Setup the Target Id"
        self.write(phy::MACSEC::MACSEC_20(), ((target >> 2) & 0xf).into())?;

        // "non-macsec access"
        let target_tmp = if target >> 2 == 1 { target & 3 } else { 0 };

        // "Trigger CSR Action - Read(16) into the CSR's and wait for complete"
        self.write(
            phy::MACSEC::MACSEC_19(),
            TryInto::<u16>::try_into(
                // VTSS_PHY_F_PAGE_MACSEC_19_CMD_BIT
                (1 << 15) |
            // VTSS_PHY_F_PAGE_MACSEC_19_READ
            (1 << 14) |
            // VTSS_PHY_F_PAGE_MACSEC_19_TARGET
            ((u32::from(target_tmp) & 0b11) << 12) |
            // VTSS_PHY_F_PAGE_MACSEC_19_CSR_REG_ADDR
            (csr_reg_addr & 0x3fff),
            )
            .unwrap()
            .into(),
        )?;

        self.macsec_wait(19)?;
        let lsb = self.read(phy::MACSEC::MACSEC_CSR_DATA_LSB())?;
        let msb = self.read(phy::MACSEC::MACSEC_CSR_DATA_MSB())?;
        Ok((u32::from(msb.0) << 16) | u32::from(lsb.0))
    }

    /// `vtss_phy_macsec_csr_wr_private`
    fn macsec_csr_write(
        &mut self,
        target: u16,
        csr_reg_addr: u32,
        value: u32,
    ) -> Result<(), VscError> {
        // "Wait for MACSEC register access"
        self.macsec_wait(19)?;

        self.write(phy::MACSEC::MACSEC_20(), ((target >> 2) & 0xf).into())?;

        // "non-macsec access"
        let target_tmp = if target >> 2 == 1 || target >> 2 == 3 {
            target
        } else {
            0
        };

        self.write(phy::MACSEC::MACSEC_CSR_DATA_LSB(), (value as u16).into())?;
        self.write(
            phy::MACSEC::MACSEC_CSR_DATA_MSB(),
            ((value >> 16) as u16).into(),
        )?;

        // "Trigger CSR Action"
        self.write(
            phy::MACSEC::MACSEC_19(),
            TryInto::<u16>::try_into(
                // VTSS_PHY_F_PAGE_MACSEC_19_CMD_BIT
                (1 << 15) |
            // VTSS_PHY_F_PAGE_MACSEC_19_TARGET
            ((u32::from(target_tmp) & 0b11) << 12) |
            // VTSS_PHY_F_PAGE_MACSEC_19_CSR_REG_ADDR
            (csr_reg_addr & 0x3fff),
            )
            .unwrap()
            .into(),
        )?;

        self.macsec_wait(19)?;

        Ok(())
    }

    /// `vtss_phy_wait_for_macsec_command_busy`
    fn macsec_wait(&mut self, page: u32) -> Result<(), VscError> {
        // Timeout based on the SDK
        for _ in 0..255 {
            match page {
                19 => {
                    let value = self.read(phy::MACSEC::MACSEC_19())?;
                    if value.0 & (1 << 15) != 0 {
                        return Ok(());
                    }
                }
                20 => {
                    let value = self.read(phy::MACSEC::MACSEC_20())?;
                    if value.0 == 0 {
                        return Ok(());
                    }
                }
                _ => panic!("Invalid MACSEC page"),
            }
        }
        Err(VscError::MacSecWaitTimeout)
    }

    /// Helper function to combine `macsec_csr_read`, some modification,
    /// followed by `macsec_csr_write`
    fn macsec_csr_modify<F>(
        &mut self,
        target: u16,
        csr_reg_addr: u32,
        f: F,
    ) -> Result<(), VscError>
    where
        F: Fn(&mut u32),
    {
        let mut reg_val = self.macsec_csr_read(target, csr_reg_addr)?;
        f(&mut reg_val);
        self.macsec_csr_write(target, csr_reg_addr, reg_val)
    }

    /// `vtss_phy_sd1g_ib_cfg_wr_private`
    fn sd1g_ib_cfg_write(
        &mut self,
        ib_ena_cmv_term: u8,
    ) -> Result<(), VscError> {
        self.macsec_csr_modify(7, 0x13, |r| {
            *r &= !(1 << 13);
            *r |= u32::from(ib_ena_cmv_term) << 13;
        })
    }

    /// `vtss_phy_sd1g_misc_cfg_wr_private`
    fn sd1g_misc_cfg_write(
        &mut self,
        des_100fx_cpmd_mode: u8,
    ) -> Result<(), VscError> {
        self.macsec_csr_modify(7, 0x1e, |r| {
            *r &= !(1 << 9);
            *r |= u32::from(des_100fx_cpmd_mode) << 9;
        })
    }

    /// `vtss_phy_sd1g_des_cfg_wr_private`
    fn sd1g_des_cfg_write(
        &mut self,
        des_phs_ctrl: u8,
        des_mbtr_ctrl: u8,
    ) -> Result<(), VscError> {
        self.macsec_csr_modify(7, 0x12, |r| {
            *r &= !((0xf << 13) | (0x7 << 8));
            *r |= (u32::from(des_phs_ctrl) << 13)
                | (u32::from(des_mbtr_ctrl) << 8);
        })
    }

    /// `vtss_phy_sd6g_pll_cfg_wr_private`
    fn sd6g_pll_cfg_write(
        &mut self,
        pll_ena_offs: u8,
        pll_fsm_ctrl_data: u8,
        pll_fsm_ena: u8,
    ) -> Result<(), VscError> {
        let reg_val = (u32::from(pll_ena_offs) << 21)
            | (u32::from(pll_fsm_ctrl_data) << 8)
            | (u32::from(pll_fsm_ena) << 7);
        self.macsec_csr_write(7, 0x2b, reg_val)
    }

    /// `vtss_phy_sd6g_ib_cfg0_wr_private`
    fn sd6g_ib_cfg0_write(
        &mut self,
        ib_rtrm_adj: u8,
        ib_sig_det_clk_sel: u8,
        ib_reg_pat_sel_offset: u8,
        ib_cal_ena: u8,
    ) -> Result<(), VscError> {
        // "constant terms"
        let base_val = (1 << 30)
            | (1 << 29)
            | (5 << 21)
            | (1 << 19)
            | (1 << 14)
            | (1 << 12)
            | (2 << 10)
            | (1 << 5)
            | (1 << 4)
            | 7;
        // "configurable terms"
        let reg_val = base_val
            | (u32::from(ib_rtrm_adj) << 25)
            | (u32::from(ib_sig_det_clk_sel) << 16)
            | (u32::from(ib_reg_pat_sel_offset) << 8)
            | (u32::from(ib_cal_ena) << 3);
        self.macsec_csr_write(7, 0x22, reg_val)
    }

    /// `vtss_phy_sd6g_ib_cfg1_wr_private`
    fn sd6g_ib_cfg1_write(
        &mut self,
        ib_tjtag: u8,
        ib_tsdet: u8,
        ib_scaly: u8,
        ib_frc_offset: u8,
        ib_filt_offset: u8,
    ) -> Result<(), VscError> {
        // "constant terms"
        let ib_filt_val = (1 << 7) | (1 << 6) | (1 << 5);
        let ib_frc_val = (0 << 3) | (0 << 2) | (0 << 1);
        // "configurable terms"
        let reg_val = (u32::from(ib_tjtag) << 17)
            | (u32::from(ib_tsdet) << 12)
            | (u32::from(ib_scaly) << 8)
            | ib_filt_val
            | (u32::from(ib_filt_offset) << 4)
            | ib_frc_val
            | (u32::from(ib_frc_offset) << 0);
        self.macsec_csr_write(7, 0x23, reg_val)
    }

    /// `vtss_phy_sd6g_ib_cfg2_wr_private`
    fn sd6g_ib_cfg2_write(
        &mut self,
        ib_tinfv: u8,
        ib_tcalv: u8,
        ib_ureg: u8,
    ) -> Result<(), VscError> {
        // "constant terms"
        // "in theory, we should read the register and mask off bits 30:28, etc.,
        //  and/or pass in other arguments"
        let base_val = 0x0f878010;
        let reg_val = base_val
            | (u32::from(ib_tinfv) << 28)
            | (u32::from(ib_tcalv) << 5)
            | (u32::from(ib_ureg) << 0);
        self.macsec_csr_write(7, 0x24, reg_val)
    }

    /// `vtss_phy_sd6g_ib_cfg3_wr_private`
    fn sd6g_ib_cfg3_write(
        &mut self,
        ib_ini_hp: u8,
        ib_ini_mid: u8,
        ib_ini_lp: u8,
        ib_ini_offset: u8,
    ) -> Result<(), VscError> {
        let reg_val = (u32::from(ib_ini_hp) << 24)
            | (u32::from(ib_ini_mid) << 16)
            | (u32::from(ib_ini_lp) << 8)
            | (u32::from(ib_ini_offset) << 0);
        self.macsec_csr_write(7, 0x25, reg_val)
    }

    /// `vtss_phy_sd6g_ib_cfg4_wr_private`
    fn sd6g_ib_cfg4_write(
        &mut self,
        ib_max_hp: u8,
        ib_max_mid: u8,
        ib_max_lp: u8,
        ib_max_offset: u8,
    ) -> Result<(), VscError> {
        let reg_val = (u32::from(ib_max_hp) << 24)
            | (u32::from(ib_max_mid) << 16)
            | (u32::from(ib_max_lp) << 8)
            | (u32::from(ib_max_offset) << 0);
        self.macsec_csr_write(7, 0x26, reg_val)
    }
    /// `vtss_phy_sd6g_des_cfg_wr_private`
    fn sd6g_des_cfg_write(
        &mut self,
        des_phy_ctrl: u8,
        des_mbtr_ctrl: u8,
        des_bw_hyst: u8,
        des_bw_ana: u8,
        des_cpmd_sel: u8,
    ) -> Result<(), VscError> {
        let reg_val = (u32::from(des_phy_ctrl) << 13)
            | (u32::from(des_mbtr_ctrl) << 10)
            | (u32::from(des_cpmd_sel) << 8)
            | (u32::from(des_bw_hyst) << 5)
            | (u32::from(des_bw_ana) << 1);
        self.macsec_csr_write(7, 0x21, reg_val)
    }

    /// `vtss_phy_sd6g_misc_cfg_wr_private`
    fn sd6g_misc_cfg_write(&mut self, lane_rst: u8) -> Result<(), VscError> {
        self.macsec_csr_write(7, 0x3b, u32::from(lane_rst))
    }

    /// `vtss_phy_sd6g_gp_cfg_wr_private`
    fn sd6g_gp_cfg_write(&mut self, gp_cfg_val: u32) -> Result<(), VscError> {
        self.macsec_csr_write(7, 0x2e, gp_cfg_val)
    }

    /// `vtss_phy_sd6g_dft_cfg0_wr_private`
    fn sd6g_dft_cfg0_write(
        &mut self,
        prbs_sel: u8,
        test_mode: u8,
        rx_dft_ena: u8,
    ) -> Result<(), VscError> {
        let reg_val = (u32::from(prbs_sel) << 20)
            | (u32::from(test_mode) << 16)
            | (u32::from(rx_dft_ena) << 2);
        self.macsec_csr_write(7, 0x35, reg_val)
    }

    /// `vtss_phy_sd6g_dft_cfg2_wr_private`
    fn sd6g_dft_cfg2_write(
        &mut self,
        rx_ji_ampl: u8,
        rx_step_freq: u8,
        rx_ji_ena: u8,
        rx_waveform_sel: u8,
        rx_freqoff_dir: u8,
        rx_freqoff_ena: u8,
    ) -> Result<(), VscError> {
        // "configurable terms"
        let reg_val = (u32::from(rx_ji_ampl) << 8)
            | (u32::from(rx_step_freq) << 4)
            | (u32::from(rx_ji_ena) << 3)
            | (u32::from(rx_waveform_sel) << 2)
            | (u32::from(rx_freqoff_dir) << 1)
            | u32::from(rx_freqoff_ena);
        self.macsec_csr_write(7, 0x37, reg_val)
    }

    /// `vtss_phy_pll5g_cfg0_wr_private`
    fn pll5g_cfg0_write(&mut self, selbgv820: u8) -> Result<(), VscError> {
        // "in theory, we should read the register and mask off bits 26:23, or pass
        //  in other arguments"
        let base_val = 0x7036f145;
        let reg_val = base_val | (u32::from(selbgv820) << 23);
        self.macsec_csr_write(7, 0x06, reg_val)
    }

    /// `vtss_phy_sd6g_common_cfg_wr_private`
    fn sd6g_common_cfg_write(
        &mut self,
        sys_rst: u8,
        ena_lane: u8,
        // 8 for eloop, 4 for floop, 2 for iloop, 1 for ploop
        ena_loop: u8,
        // 1 for SGMII, 0 for QSGMII
        qrate: u8,
        // 1 for SGMII, 3 for QSGMII
        if_mode: u8,
        pwd_tx: u8,
    ) -> Result<(), VscError> {
        let reg_val = (u32::from(sys_rst) << 31)
            | (u32::from(ena_lane) << 18)
            | (u32::from(pwd_tx) << 16)
            | (u32::from(ena_loop) << 8)
            | (u32::from(qrate) << 6)
            | (u32::from(if_mode) << 4);
        self.macsec_csr_write(7, 0x2c, reg_val)
    }
}