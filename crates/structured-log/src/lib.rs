#![no_std]

pub const CAPACITY: usize = 32;
pub const MESSAGE_BYTES: usize = 80;
const HEADER_BYTES: usize = 32;
const RECORD_BYTES: usize = 112;
pub const IMAGE_BYTES: usize = HEADER_BYTES + CAPACITY * RECORD_BYTES;
const MAGIC: &[u8; 8] = b"MAKLOG01";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub sequence: u64,
    pub ticks: u64,
    pub pid: u64,
    pub severity: u8,
    pub length: u8,
    pub message: [u8; MESSAGE_BYTES],
}

impl Record {
    pub const EMPTY: Self = Self {
        sequence: 0,
        ticks: 0,
        pid: 0,
        severity: 0,
        length: 0,
        message: [0; MESSAGE_BYTES],
    };

    pub fn message(&self) -> &[u8] {
        &self.message[..usize::from(self.length)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Journal {
    records: [Record; CAPACITY],
    next_sequence: u64,
}

impl Journal {
    pub const fn new() -> Self {
        Self {
            records: [Record::EMPTY; CAPACITY],
            next_sequence: 1,
        }
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn record_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.sequence != 0)
            .count()
    }

    pub fn append(&mut self, ticks: u64, pid: u64, severity: u8, message: &[u8]) -> Option<u64> {
        if severity > 7 || message.is_empty() || message.len() > MESSAGE_BYTES {
            return None;
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1)?;
        let index = (sequence as usize - 1) % CAPACITY;
        let mut record = Record::EMPTY;
        record.sequence = sequence;
        record.ticks = ticks;
        record.pid = pid;
        record.severity = severity;
        record.length = message.len() as u8;
        record.message[..message.len()].copy_from_slice(message);
        self.records[index] = record;
        Some(sequence)
    }

    pub fn record(&self, sequence: u64) -> Option<Record> {
        if sequence == 0 {
            return None;
        }
        let record = self.records[(sequence as usize - 1) % CAPACITY];
        (record.sequence == sequence).then_some(record)
    }

    pub fn encode(&self) -> [u8; IMAGE_BYTES] {
        let mut output = [0u8; IMAGE_BYTES];
        output[..8].copy_from_slice(MAGIC);
        put_u32(&mut output, 8, 1);
        put_u32(&mut output, 12, CAPACITY as u32);
        put_u64(&mut output, 16, self.next_sequence);
        put_u32(&mut output, 24, self.record_count() as u32);
        for (index, record) in self.records.iter().enumerate() {
            let offset = HEADER_BYTES + index * RECORD_BYTES;
            put_u64(&mut output, offset, record.sequence);
            put_u64(&mut output, offset + 8, record.ticks);
            put_u64(&mut output, offset + 16, record.pid);
            output[offset + 24] = record.severity;
            output[offset + 25] = record.length;
            output[offset + 32..offset + 32 + MESSAGE_BYTES].copy_from_slice(&record.message);
        }
        let crc = image_crc(&output);
        put_u32(&mut output, 28, crc);
        output
    }

    pub fn decode(input: &[u8]) -> Result<Self, DecodeError> {
        if input.len() != IMAGE_BYTES
            || input[..8] != *MAGIC
            || get_u32(input, 8) != 1
            || get_u32(input, 12) != CAPACITY as u32
        {
            return Err(DecodeError::Header);
        }
        if image_crc(input) != get_u32(input, 28) {
            return Err(DecodeError::Checksum);
        }
        let next_sequence = get_u64(input, 16);
        if next_sequence == 0 {
            return Err(DecodeError::Record);
        }
        let mut journal = Self::new();
        journal.next_sequence = next_sequence;
        let mut count = 0usize;
        let expected_count = usize::try_from(next_sequence - 1)
            .unwrap_or(usize::MAX)
            .min(CAPACITY);
        let earliest = next_sequence.saturating_sub(CAPACITY as u64).max(1);
        for index in 0..CAPACITY {
            let offset = HEADER_BYTES + index * RECORD_BYTES;
            let sequence = get_u64(input, offset);
            if sequence == 0 {
                if input[offset..offset + RECORD_BYTES]
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(DecodeError::Record);
                }
                continue;
            }
            let severity = input[offset + 24];
            let length = input[offset + 25];
            if sequence < earliest
                || sequence >= next_sequence
                || (sequence as usize - 1) % CAPACITY != index
                || severity > 7
                || length == 0
                || usize::from(length) > MESSAGE_BYTES
                || input[offset + 26..offset + 32]
                    .iter()
                    .any(|byte| *byte != 0)
            {
                return Err(DecodeError::Record);
            }
            let mut record = Record::EMPTY;
            record.sequence = sequence;
            record.ticks = get_u64(input, offset + 8);
            record.pid = get_u64(input, offset + 16);
            record.severity = severity;
            record.length = length;
            record
                .message
                .copy_from_slice(&input[offset + 32..offset + 32 + MESSAGE_BYTES]);
            if record.message[usize::from(length)..]
                .iter()
                .any(|byte| *byte != 0)
            {
                return Err(DecodeError::Record);
            }
            journal.records[index] = record;
            count += 1;
        }
        if count != expected_count || count != get_u32(input, 24) as usize {
            return Err(DecodeError::Record);
        }
        Ok(journal)
    }
}

impl Default for Journal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    Header,
    Checksum,
    Record,
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(input[offset..offset + 4].try_into().unwrap())
}

fn get_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(input[offset..offset + 8].try_into().unwrap())
}

fn image_crc(bytes: &[u8]) -> u32 {
    let crc = crc32_update(0xffff_ffffu32, &bytes[..28]);
    !crc32_update(crc, &bytes[32..])
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wraps_and_preserves_latest_records() {
        let mut journal = Journal::new();
        for value in 0..40u64 {
            let message = [value as u8; 3];
            assert_eq!(journal.append(100 + value, 7, 5, &message), Some(value + 1));
        }
        let decoded = Journal::decode(&journal.encode()).unwrap();
        assert_eq!(decoded.next_sequence(), 41);
        assert_eq!(decoded.record_count(), CAPACITY);
        assert!(decoded.record(8).is_none());
        let latest = decoded.record(40).unwrap();
        assert_eq!(latest.ticks, 139);
        assert_eq!(latest.pid, 7);
        assert_eq!(latest.severity, 5);
        assert_eq!(latest.message(), &[39; 3]);
    }

    #[test]
    fn rejects_header_checksum_and_record_corruption() {
        let mut journal = Journal::new();
        journal.append(1, 1, 2, b"online").unwrap();
        let image = journal.encode();

        let mut bad = image;
        bad[0] ^= 1;
        assert_eq!(Journal::decode(&bad), Err(DecodeError::Header));

        let mut bad = image;
        bad[HEADER_BYTES + 32] ^= 1;
        assert_eq!(Journal::decode(&bad), Err(DecodeError::Checksum));

        let mut bad = image;
        bad[HEADER_BYTES + 25] = 81;
        let crc = image_crc(&bad);
        put_u32(&mut bad, 28, crc);
        assert_eq!(Journal::decode(&bad), Err(DecodeError::Record));

        let mut bad = image;
        put_u64(&mut bad, 16, 4);
        let crc = image_crc(&bad);
        put_u32(&mut bad, 28, crc);
        assert_eq!(Journal::decode(&bad), Err(DecodeError::Record));
    }

    #[test]
    fn rejects_invalid_append_without_consuming_sequence() {
        let mut journal = Journal::new();
        assert_eq!(journal.append(1, 1, 8, b"bad"), None);
        assert_eq!(journal.append(1, 1, 1, &[]), None);
        assert_eq!(journal.next_sequence(), 1);
    }
}
