// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// NOTE: This DER blob, offsets & lengths is mostly generated code. This was
// accomplished by creating a certificate with the desired structure (including
// DICE specific extensions and policy) using the openssl ca tools. The fields
// that we need to operate on were identified and their offsets recorded. The
// values in these regions (signatureValue, serialNumber, issuer, subject,
// validity etc) are then removed.
//
// TODO: generate cert template DER from ASN.1 & text config
#[allow(dead_code)]
pub const SIZE: usize = 531;
#[allow(dead_code)]
pub const SERIAL_NUMBER_START: usize = 15;
#[allow(dead_code)]
pub const SERIAL_NUMBER_END: usize = 16;
#[allow(dead_code)]
pub const ISSUER_SN_START: usize = 169;
#[allow(dead_code)]
pub const ISSUER_SN_END: usize = 181;
#[allow(dead_code)]
pub const SN_LENGTH: usize = 12;
#[allow(dead_code)]
pub const NOTBEFORE_START: usize = 185;
#[allow(dead_code)]
pub const NOTBEFORE_END: usize = 198;
#[allow(dead_code)]
pub const NOTBEFORE_LENGTH: usize = 13;
#[allow(dead_code)]
pub const SUBJECT_SN_START: usize = 361;
#[allow(dead_code)]
pub const SUBJECT_SN_END: usize = 373;
#[allow(dead_code)]
pub const PUB_START: usize = 385;
#[allow(dead_code)]
pub const PUB_END: usize = 417;
#[allow(dead_code)]
pub const SIG_START: usize = 467;
#[allow(dead_code)]
pub const SIG_END: usize = 531;
#[allow(dead_code)]
pub const SIGNDATA_START: usize = 4;
#[allow(dead_code)]
pub const SIGNDATA_END: usize = 457;
#[allow(dead_code)]
pub const SIGNDATA_LENGTH: usize = 453;
pub const CERT_TMPL: [u8; 531] = [
    0x30, 0x82, 0x02, 0x0f, 0x30, 0x82, 0x01, 0xc1, 0xa0, 0x03, 0x02, 0x01,
    0x02, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x30,
    0x81, 0x9b, 0x31, 0x0b, 0x30, 0x09, 0x06, 0x03, 0x55, 0x04, 0x06, 0x13,
    0x02, 0x55, 0x53, 0x31, 0x13, 0x30, 0x11, 0x06, 0x03, 0x55, 0x04, 0x08,
    0x0c, 0x0a, 0x43, 0x61, 0x6c, 0x69, 0x66, 0x6f, 0x72, 0x6e, 0x69, 0x61,
    0x31, 0x13, 0x30, 0x11, 0x06, 0x03, 0x55, 0x04, 0x07, 0x0c, 0x0a, 0x45,
    0x6d, 0x65, 0x72, 0x79, 0x76, 0x69, 0x6c, 0x6c, 0x65, 0x31, 0x1f, 0x30,
    0x1d, 0x06, 0x03, 0x55, 0x04, 0x0a, 0x0c, 0x16, 0x4f, 0x78, 0x69, 0x64,
    0x65, 0x20, 0x43, 0x6f, 0x6d, 0x70, 0x75, 0x74, 0x65, 0x72, 0x20, 0x43,
    0x6f, 0x6d, 0x70, 0x61, 0x6e, 0x79, 0x31, 0x16, 0x30, 0x14, 0x06, 0x03,
    0x55, 0x04, 0x0b, 0x0c, 0x0d, 0x4d, 0x61, 0x6e, 0x75, 0x66, 0x61, 0x63,
    0x74, 0x75, 0x72, 0x69, 0x6e, 0x67, 0x31, 0x12, 0x30, 0x10, 0x06, 0x03,
    0x55, 0x04, 0x03, 0x0c, 0x09, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x2d,
    0x69, 0x64, 0x31, 0x15, 0x30, 0x13, 0x06, 0x03, 0x55, 0x04, 0x05, 0x13,
    0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x30, 0x20, 0x17, 0x0d, 0x32, 0x32, 0x30, 0x37, 0x33, 0x31, 0x31,
    0x36, 0x33, 0x33, 0x33, 0x37, 0x5a, 0x18, 0x0f, 0x39, 0x39, 0x39, 0x39,
    0x31, 0x32, 0x33, 0x31, 0x32, 0x33, 0x35, 0x39, 0x35, 0x39, 0x5a, 0x30,
    0x81, 0x9b, 0x31, 0x0b, 0x30, 0x09, 0x06, 0x03, 0x55, 0x04, 0x06, 0x13,
    0x02, 0x55, 0x53, 0x31, 0x13, 0x30, 0x11, 0x06, 0x03, 0x55, 0x04, 0x08,
    0x0c, 0x0a, 0x43, 0x61, 0x6c, 0x69, 0x66, 0x6f, 0x72, 0x6e, 0x69, 0x61,
    0x31, 0x13, 0x30, 0x11, 0x06, 0x03, 0x55, 0x04, 0x07, 0x0c, 0x0a, 0x45,
    0x6d, 0x65, 0x72, 0x79, 0x76, 0x69, 0x6c, 0x6c, 0x65, 0x31, 0x1f, 0x30,
    0x1d, 0x06, 0x03, 0x55, 0x04, 0x0a, 0x0c, 0x16, 0x4f, 0x78, 0x69, 0x64,
    0x65, 0x20, 0x43, 0x6f, 0x6d, 0x70, 0x75, 0x74, 0x65, 0x72, 0x20, 0x43,
    0x6f, 0x6d, 0x70, 0x61, 0x6e, 0x79, 0x31, 0x16, 0x30, 0x14, 0x06, 0x03,
    0x55, 0x04, 0x0b, 0x0c, 0x0d, 0x4d, 0x61, 0x6e, 0x75, 0x66, 0x61, 0x63,
    0x74, 0x75, 0x72, 0x69, 0x6e, 0x67, 0x31, 0x12, 0x30, 0x10, 0x06, 0x03,
    0x55, 0x04, 0x03, 0x0c, 0x09, 0x64, 0x65, 0x76, 0x69, 0x63, 0x65, 0x2d,
    0x69, 0x64, 0x31, 0x15, 0x30, 0x13, 0x06, 0x03, 0x55, 0x04, 0x05, 0x13,
    0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa3, 0x26, 0x30,
    0x24, 0x30, 0x12, 0x06, 0x03, 0x55, 0x1d, 0x13, 0x01, 0x01, 0xff, 0x04,
    0x08, 0x30, 0x06, 0x01, 0x01, 0xff, 0x02, 0x01, 0x00, 0x30, 0x0e, 0x06,
    0x03, 0x55, 0x1d, 0x0f, 0x01, 0x01, 0xff, 0x04, 0x04, 0x03, 0x02, 0x01,
    0x86, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x41, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00,
];
