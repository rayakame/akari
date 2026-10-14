// Writes protobuf for tests, so every case is readable bytes, not captured ones.

pub(crate) enum Wire {
    Varint(u64),
    Fixed64(u64),
    Bytes(Vec<u8>),
    Fixed32(u32),
}

pub(crate) fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn field(number: u64, wire: Wire) -> Vec<u8> {
    let (kind, payload) = match wire {
        Wire::Varint(value) => (0, varint(value)),
        Wire::Fixed64(value) => (1, value.to_le_bytes().to_vec()),
        Wire::Bytes(bytes) => (2, [varint(bytes.len() as u64), bytes].concat()),
        Wire::Fixed32(value) => (5, value.to_le_bytes().to_vec()),
    };
    [varint(number << 3 | kind), payload].concat()
}
