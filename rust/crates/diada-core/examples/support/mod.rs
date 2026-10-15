/// Portable raw snapshots retain their original corpus index. Unpacked oracle
/// dumps are accepted by the same validators during regeneration.
pub fn cases(bytes: &[u8], snapshot_size: usize) -> Box<dyn Iterator<Item = (usize, &[u8])> + '_> {
    if bytes.starts_with(b"DIREF320") {
        assert_eq!(
            usize::from(u16::from_le_bytes(bytes[8..10].try_into().unwrap())),
            snapshot_size
        );
        let count = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 16 + count * (snapshot_size + 4));
        Box::new(bytes[16..].chunks_exact(snapshot_size + 4).map(|record| {
            (
                u32::from_le_bytes(record[..4].try_into().unwrap()) as usize,
                &record[4..],
            )
        }))
    } else {
        assert_eq!(bytes.len() % snapshot_size, 0);
        Box::new(bytes.chunks_exact(snapshot_size).enumerate())
    }
}
