//! Minimal binary property list (bplist00) codec: the subset AirPlay 2 uses
//! (dict, array, string, unsigned int, real, bool, data). Decoding takes what a
//! receiver on the network sends, so every length and offset in it is checked.

use std::cell::Cell;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(u64),
    Real(f64),
    String(String),
    Data(Vec<u8>),
    Array(Vec<Value>),
    Dict(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn index(&self, i: usize) -> Option<&Value> {
        match self {
            Value::Array(a) => a.get(i),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
}

/// Build a dict from `(key, value)` pairs.
pub fn dict<const N: usize>(items: [(&str, Value); N]) -> Value {
    Value::Dict(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

// ---------------------------------------------------------------- encode

pub fn encode(root: &Value) -> Vec<u8> {
    // Flatten into an object table; each container stores child object refs.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    flatten(root, &mut objects);
    let n = objects.len();
    let ref_size = if n < 256 { 1 } else { 2 };
    // Refs were written as u16 placeholders; shrink to ref_size.
    let objects: Vec<Vec<u8>> = objects.into_iter().map(|o| fix_refs(o, ref_size)).collect();

    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::with_capacity(n);
    for o in &objects {
        offsets.push(out.len());
        out.extend_from_slice(o);
    }
    let table_offset = out.len();
    let offset_size = int_size(table_offset as u64);
    for off in offsets {
        out.extend_from_slice(&(off as u64).to_be_bytes()[8 - offset_size..]);
    }
    out.extend_from_slice(&[0; 6]);
    out.push(offset_size as u8);
    out.push(ref_size as u8);
    out.extend_from_slice(&(n as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes()); // top object
    out.extend_from_slice(&(table_offset as u64).to_be_bytes());
    out
}

fn int_size(v: u64) -> usize {
    match v {
        0..=0xff => 1,
        0x100..=0xffff => 2,
        0x1_0000..=0xffff_ffff => 4,
        _ => 8,
    }
}

fn marker_len(marker: u8, len: usize, out: &mut Vec<u8>) {
    if len < 15 {
        out.push(marker | len as u8);
    } else {
        out.push(marker | 0x0f);
        out.extend(int_bytes(len as u64));
    }
}

fn int_bytes(v: u64) -> Vec<u8> {
    let size = int_size(v);
    let mut o = vec![0x10 | size.trailing_zeros() as u8];
    o.extend_from_slice(&v.to_be_bytes()[8 - size..]);
    o
}

// Container objects temporarily hold refs as: 0xFF marker byte then u16 BE per ref,
// tagged so fix_refs can re-encode them once the final ref size is known.
const REF_TAG: u8 = 0xff;

fn flatten(v: &Value, objects: &mut Vec<Vec<u8>>) -> usize {
    let index = objects.len();
    objects.push(Vec::new());
    let bytes = match v {
        Value::Bool(b) => vec![if *b { 0x09 } else { 0x08 }],
        Value::Int(n) => int_bytes(*n),
        Value::Real(f) => {
            let mut o = vec![0x23];
            o.extend_from_slice(&f.to_be_bytes());
            o
        }
        Value::String(s) if s.is_ascii() => {
            let mut o = Vec::new();
            marker_len(0x50, s.len(), &mut o);
            o.extend_from_slice(s.as_bytes());
            o
        }
        Value::String(s) => {
            let units: Vec<u16> = s.encode_utf16().collect();
            let mut o = Vec::new();
            marker_len(0x60, units.len(), &mut o);
            units.iter().for_each(|u| o.extend_from_slice(&u.to_be_bytes()));
            o
        }
        Value::Data(d) => {
            let mut o = Vec::new();
            marker_len(0x40, d.len(), &mut o);
            o.extend_from_slice(d);
            o
        }
        Value::Array(items) => {
            let refs: Vec<usize> = items.iter().map(|i| flatten(i, objects)).collect();
            let mut o = Vec::new();
            marker_len(0xa0, refs.len(), &mut o);
            o.push(REF_TAG);
            refs.iter().for_each(|r| o.extend_from_slice(&(*r as u16).to_be_bytes()));
            o
        }
        Value::Dict(items) => {
            let keys: Vec<usize> = items.iter().map(|(k, _)| flatten(&Value::String(k.clone()), objects)).collect();
            let vals: Vec<usize> = items.iter().map(|(_, v)| flatten(v, objects)).collect();
            let mut o = Vec::new();
            marker_len(0xd0, keys.len(), &mut o);
            o.push(REF_TAG);
            keys.iter().chain(&vals).for_each(|r| o.extend_from_slice(&(*r as u16).to_be_bytes()));
            o
        }
    };
    objects[index] = bytes;
    index
}

fn fix_refs(o: Vec<u8>, ref_size: usize) -> Vec<u8> {
    let marker = o[0] & 0xf0;
    if marker != 0xa0 && marker != 0xd0 {
        return o;
    }
    let tag = if o[0] & 0x0f != 0x0f { 1 } else { 2 + (1usize << (o[1] & 0x0f)) };
    debug_assert_eq!(o[tag], REF_TAG);
    let mut out = o[..tag].to_vec();
    for r in o[tag + 1..].chunks_exact(2) {
        out.extend_from_slice(&r[2 - ref_size..]);
    }
    out
}

// ---------------------------------------------------------------- decode

/// Deepest nesting, and most objects read (a list may refer to the same object
/// many times over): far above what a receiver sends.
const MAX_DEPTH: usize = 32;
const MAX_READS: usize = 10_000;

pub fn decode(data: &[u8]) -> Result<Value, String> {
    if data.len() < 40 || &data[..8] != b"bplist00" {
        return Err("not a binary plist".into());
    }
    let t = &data[data.len() - 32..];
    let size = |b: &[u8]| usize::try_from(u64::from_be_bytes(b.try_into().expect("8 bytes"))).map_err(|_| "plist: too big");
    let (offset_size, ref_size) = (t[6] as usize, t[7] as usize);
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) {
        return Err("plist: bad trailer".into());
    }
    let p = Parser { data, offset_size, ref_size, num: size(&t[8..16])?, table: size(&t[24..32])?, reads: Cell::new(0) };
    p.object(size(&t[16..24])?, 0)
}

struct Parser<'a> {
    data: &'a [u8],
    offset_size: usize,
    ref_size: usize,
    num: usize,
    table: usize,
    /// Objects read so far, against [`MAX_READS`].
    reads: Cell<usize>,
}

const TRUNCATED: &str = "plist: truncated";

impl Parser<'_> {
    /// The big-endian number of `size` (1 to 8) bytes at `at`.
    fn uint(&self, at: usize, size: usize) -> Result<u64, String> {
        if !(1..=8).contains(&size) {
            return Err(format!("plist: {size}-byte number"));
        }
        let b = self.data.get(at..at.checked_add(size).ok_or(TRUNCATED)?).ok_or(TRUNCATED)?;
        Ok(b.iter().fold(0u64, |acc, &x| (acc << 8) | x as u64))
    }

    /// The `i`th of `size`-byte entries starting at `base`, as a number.
    fn entry(&self, base: usize, i: usize, size: usize) -> Result<usize, String> {
        let at = i.checked_mul(size).and_then(|o| o.checked_add(base)).ok_or(TRUNCATED)?;
        usize::try_from(self.uint(at, size)?).map_err(|_| "plist: bad number".into())
    }

    fn offset(&self, index: usize) -> Result<usize, String> {
        if index >= self.num {
            return Err("plist: bad ref".into());
        }
        self.entry(self.table, index, self.offset_size)
    }

    /// `len` items of `width` bytes at `p`.
    fn bytes(&self, p: usize, len: usize, width: usize) -> Result<&[u8], String> {
        let end = len.checked_mul(width).and_then(|n| n.checked_add(p)).ok_or(TRUNCATED)?;
        self.data.get(p..end).ok_or_else(|| TRUNCATED.into())
    }

    /// Returns (length, position of payload).
    fn length(&self, at: usize) -> Result<(usize, usize), String> {
        let low = (self.data[at] & 0x0f) as usize;
        if low != 0x0f {
            return Ok((low, at + 1));
        }
        let size = 1usize << (self.data.get(at + 1).ok_or(TRUNCATED)? & 0x0f);
        let len = usize::try_from(self.uint(at + 2, size)?).map_err(|_| "plist: bad length")?;
        Ok((len, at + 2 + size))
    }

    /// The object `ref_size` bytes at `p` refer to, entry `i`.
    fn child(&self, p: usize, i: usize, depth: usize) -> Result<Value, String> {
        self.object(self.entry(p, i, self.ref_size)?, depth + 1)
    }

    fn object(&self, index: usize, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("plist: too deep".into());
        }
        self.reads.set(self.reads.get() + 1);
        if self.reads.get() > MAX_READS {
            return Err("plist: too many objects".into());
        }
        let at = self.offset(index)?;
        let marker = *self.data.get(at).ok_or(TRUNCATED)?;
        Ok(match marker & 0xf0 {
            0x00 => Value::Bool(marker == 0x09),
            0x10 => Value::Int(self.uint(at + 1, 1 << (marker & 0x0f))?),
            0x20 => {
                let size = 1 << (marker & 0x0f);
                let bits = self.uint(at + 1, size)?;
                Value::Real(if size == 4 { f32::from_bits(bits as u32) as f64 } else { f64::from_bits(bits) })
            }
            0x40 => {
                let (len, p) = self.length(at)?;
                Value::Data(self.bytes(p, len, 1)?.to_vec())
            }
            0x50 => {
                let (len, p) = self.length(at)?;
                Value::String(String::from_utf8_lossy(self.bytes(p, len, 1)?).into_owned())
            }
            0x60 => {
                let (len, p) = self.length(at)?;
                let units: Vec<u16> = self.bytes(p, len, 2)?.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                Value::String(String::from_utf16_lossy(&units))
            }
            0xa0 => {
                let (len, p) = self.length(at)?;
                self.bytes(p, len, self.ref_size)?;
                (0..len).map(|i| self.child(p, i, depth)).collect::<Result<_, _>>().map(Value::Array)?
            }
            0xd0 => {
                let (len, p) = self.length(at)?;
                self.bytes(p, len, 2 * self.ref_size)?;
                let pair = |i| {
                    let Value::String(k) = self.child(p, i, depth)? else { return Err("plist: non-string key".to_string()) };
                    Ok((k, self.child(p, len + i, depth)?))
                };
                (0..len).map(pair).collect::<Result<_, _>>().map(Value::Dict)?
            }
            _ => return Err(format!("plist: unsupported marker {marker:#x}")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let v = dict([
            ("deviceID", Value::String("AA:BB:CC:DD:EE:FF".into())),
            ("timingPort", Value::Int(6002)),
            ("big", Value::Int(0x1_0000_0000)),
            ("ok", Value::Bool(true)),
            ("shk", Value::Data(vec![7; 32])),
            ("long", Value::String("x".repeat(40))),
            ("streams", Value::Array(vec![dict([("type", Value::Int(96)), ("name", Value::String("dåst".into()))])])),
        ]);
        let bytes = encode(&v);
        assert_eq!(&bytes[..8], b"bplist00");
        assert_eq!(decode(&bytes).unwrap(), v);
        assert_eq!(
            decode(&bytes).unwrap().get("streams").and_then(|s| s.index(0)).and_then(|s| s.get("type")).and_then(Value::as_u64),
            Some(96)
        );
    }

    #[test]
    fn hostile_input_is_an_error_not_a_crash() {
        let good = encode(&dict([("volume", Value::Real(-15.0)), ("list", Value::Array(vec![Value::Int(1); 3]))]));
        // Every byte set to every value, in turn: never a panic.
        for at in 8..good.len() {
            for byte in [0x00, 0x0f, 0x7f, 0xff] {
                let mut bad = good.clone();
                bad[at] = byte;
                let _ = decode(&bad);
            }
        }
        // Lists of 1000 refs to the next list, three deep: a billion reads if each
        // ref were followed every time; refused instead.
        const REFS: u16 = 1000;
        let mut wide = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        for next in 1u8..=3 {
            offsets.push(wide.len() as u16);
            wide.extend([0xaf, 0x11]);
            wide.extend(REFS.to_be_bytes());
            wide.extend(std::iter::repeat_n(next, REFS as usize));
        }
        offsets.push(wide.len() as u16);
        wide.extend([0x10, 0x01]);
        let table = wide.len() as u64;
        offsets.iter().for_each(|o| wide.extend(o.to_be_bytes()));
        // Trailer: 2-byte offsets, 1-byte refs, 4 objects, the first on top.
        wide.extend([0, 0, 0, 0, 0, 0, 2, 1]);
        wide.extend(4u64.to_be_bytes());
        wide.extend(0u64.to_be_bytes());
        wide.extend(table.to_be_bytes());
        assert_eq!(decode(&wide), Err("plist: too many objects".into()));
    }

    #[test]
    fn decodes_python_plistlib_output() {
        // plistlib.dumps({'eventPort': 7001, 'timingPort': 0}, fmt=FMT_BINARY)
        let bytes = [
            0x62, 0x70, 0x6c, 0x69, 0x73, 0x74, 0x30, 0x30, 0xd2, 0x01, 0x02, 0x03, 0x04, 0x59, 0x65, 0x76, 0x65, 0x6e, 0x74, 0x50, 0x6f,
            0x72, 0x74, 0x5a, 0x74, 0x69, 0x6d, 0x69, 0x6e, 0x67, 0x50, 0x6f, 0x72, 0x74, 0x11, 0x1b, 0x59, 0x10, 0x00, 0x08, 0x0d, 0x17,
            0x22, 0x25, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x27,
        ];
        let v = decode(&bytes).unwrap();
        assert_eq!(v.get("eventPort").and_then(Value::as_u64), Some(7001));
        assert_eq!(v.get("timingPort").and_then(Value::as_u64), Some(0));
    }
}
