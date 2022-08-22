// MIT/APACHE2.0
// This file is derived from https://github.com/iohe/sensirion-hdlc/
//
// Copyright (C) 2019 Ioan Herisanu
// Copyright (C) 2022 Benjamin Brittain

#![no_std]

/// Frame END. Byte that marks the beginning and end of a packet
const FEND: u8 = 0x7E;

/// Frame ESCape. Byte that marks the start of a swap byte
const FESC: u8 = 0x7D;

/// Trade Frame END. Byte that is substituted for the FEND byte
const TFEND: u8 = 0x5E;

/// Trade Frame ESCape. Byte that is substituted for the FESC byte
const TFESC: u8 = 0x5D;

/// Original Byte 1. Byte that will be substituted for the TFOB1 byte
const OB1: u8 = 0x11;

/// Trade Frame ESCape. Byte that is substituted for the Original Byte 1
const TFOB1: u8 = 0x31;

/// Original Byte 2. Byte that is substituted for the TFOB2 byte
const OB2: u8 = 0x13;

/// Trade Frame Original Byte 2. Byte that is substituted for the Original Byte 2
const TFOB2: u8 = 0x33;

/// Calculate the SHDLC checksum
pub fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, x| acc.wrapping_add(*x)) ^ 0xFFu8
}

pub fn encode(data: &[u8], output: &mut [u8]) -> Result<(), HDLCError> {
    if data.len() > 260 {
        return Err(HDLCError::TooMuchData);
    }

    // Iterator over the input that allows peeking
    let input_iter = data.iter();

    let mut out_idx = 0;
    let mut push = |value| {
        if out_idx > output.len() {
            return Err(HDLCError::OutBufferTooSmall);
        }
        output[out_idx] = value;
        out_idx += 1;
        Ok(())
    };

    // Push initial FEND
    push(FEND)?;

    // Loop over every byte of the message
    for value in input_iter {
        match *value {
            // FEND , FESC, ob1 and ob2
            val if val == FESC => {
                push(FESC)?;
                push(TFESC)?;
            }
            val if val == FEND => {
                push(FESC)?;
                push(TFEND)?;
            }
            val if val == OB1 => {
                push(FESC)?;
                push(TFOB1)?;
            }
            val if val == OB2 => {
                push(FESC)?;
                push(TFOB2)?;
            }
            // Handle any other bytes
            _ => push(*value)?,
        }
    }

    // Adds checksum
    // TODO could do this as we go over the bytes the first time too
    push(calculate_checksum(&data))?;

    // Push final FEND
    push(FEND)?;

    Ok(())
}

pub fn decode(input: &[u8], output: &mut [u8]) -> Result<(), HDLCError> {
    if input.len() < 4 {
        return Err(HDLCError::TooFewData);
    }

    if input.len() > 1000 {
        return Err(HDLCError::TooMuchData);
    }

    // Verify input begins with a FEND
    if input[0] != FEND {
        return Err(HDLCError::MissingFirstFend);
    }
    // Verify input ends with a FEND
    if input[input.len() - 1] != FEND {
        return Err(HDLCError::MissingFinalFend);
    }

    // Iterator over the input that allows peeking
    let mut input_iter = input[1..input.len() - 1].iter().peekable();

    let mut out_idx = 0;
    let mut push = |value| {
        if out_idx > output.len() {
            return Err(HDLCError::OutBufferTooSmall);
        }
        output[out_idx] = value;
        out_idx += 1;
        Ok(())
    };

    // Loop over every byte of the message
    while let Some(value) = input_iter.next() {
        match *value {
            // Handle a FESC
            val if val == FESC => match input_iter.next() {
                Some(&val) if val == TFEND => push(FEND)?,
                Some(&val) if val == TFESC => push(FESC)?,
                Some(&val) if val == TFOB1 => push(OB1)?,
                Some(&val) if val == TFOB2 => push(OB2)?,
                _ => return Err(HDLCError::MissingTradeChar),
            },
            // Handle a FEND
            val if val == FEND => {
                return Err(HDLCError::FendCharInData);
            }
            // Handle any other bytes
            _ => push(*value)?,
        }
    }

    if output.len() > 260 {
        return Err(HDLCError::TooMuchDecodedData);
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
/// Common error for HDLC actions.
pub enum HDLCError {
    /// Catches duplicate special characters.
    DuplicateSpecialChar,
    /// Catches a random sync char in the data.
    FendCharInData,
    /// Catches a random swap char, `fesc`, in the data with no `tfend` or `tfesc`.
    MissingTradeChar,
    /// No first fend on the message.
    MissingFirstFend,
    /// No final fend on the message.
    MissingFinalFend,
    /// Too much data to be converted into a SHDLC frame.
    TooMuchData,
    /// Too few data to be converted from a SHDLC frame.
    TooFewData,
    /// Checksum for decoded Frame is invalid.
    InvalidChecksum,
    /// More than 259 bytes resulted after decoding SHDLC frame.
    TooMuchDecodedData,
    /// Provided Out buffer is too small to decode.
    OutBufferTooSmall
}
