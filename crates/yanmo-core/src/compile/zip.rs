//! 最小 ZIP 写入器：`.docx` 本质上就是一个 ZIP 包。
//!
//! 三条选择，都是为了这个项目在乎的东西：
//!
//! 1. **不压缩（STORE）**：同一次编译渲染两遍必须**逐字节一样**（导出的幂等铁律）。
//!    压缩输出会随压缩库版本变化而变，STORE 直接把这条钉死；投稿包里就是几万字的 XML，
//!    体积在几百 KB 量级，不值得为它冒"每次编译字节都不一样"的险。
//! 2. **零依赖**：不引第三方打包/压缩库——核心要能被审计，也不要强传染依赖。
//! 3. **不留时间戳**：DOS 时间写死 1980-01-01，别让"什么时候编的"混进字节里。
//!
//! 只实现到"够用且正确"：STORE、无目录、无加密、无 ZIP64（单文件与总大小都在 4GB 以内）。

/// 一个要打进包里的文件。
pub(crate) struct Entry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

/// 局部文件头 + 中央目录里都要写的"1980-01-01 00:00:00"（DOS 时间格式）。
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 0x0021;

/// 把若干文件打成一个 ZIP（STORE）。**同样的输入永远得到同样的字节**。
pub(crate) fn write_stored(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::with_capacity(entries.len());

    for entry in entries {
        let name = entry.name.as_bytes();
        let crc = crc32(entry.data);
        let size = entry.data.len() as u32;
        offsets.push(out.len() as u32);

        put_u32(&mut out, 0x0403_4b50); // 局部文件头签名
        put_u16(&mut out, 20); // 需要的解压版本 2.0
        put_u16(&mut out, 0x0800); // 文件名按 UTF-8 解释
        put_u16(&mut out, 0); // 压缩方式：0 = 不压缩
        put_u16(&mut out, DOS_TIME);
        put_u16(&mut out, DOS_DATE);
        put_u32(&mut out, crc);
        put_u32(&mut out, size); // 压缩后大小（= 原大小）
        put_u32(&mut out, size);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0); // 扩展字段长度
        out.extend_from_slice(name);
        out.extend_from_slice(entry.data);
    }

    // 中央目录
    let directory_at = out.len() as u32;
    for (entry, offset) in entries.iter().zip(&offsets) {
        let name = entry.name.as_bytes();
        let crc = crc32(entry.data);
        let size = entry.data.len() as u32;
        put_u32(&mut out, 0x0201_4b50);
        put_u16(&mut out, 20); // 由 2.0 版工具创建
        put_u16(&mut out, 20);
        put_u16(&mut out, 0x0800);
        put_u16(&mut out, 0);
        put_u16(&mut out, DOS_TIME);
        put_u16(&mut out, DOS_DATE);
        put_u32(&mut out, crc);
        put_u32(&mut out, size);
        put_u32(&mut out, size);
        put_u16(&mut out, name.len() as u16);
        put_u16(&mut out, 0); // 扩展字段
        put_u16(&mut out, 0); // 注释
        put_u16(&mut out, 0); // 起始磁盘号
        put_u16(&mut out, 0); // 内部属性
        put_u32(&mut out, 0); // 外部属性
        put_u32(&mut out, *offset);
        out.extend_from_slice(name);
    }
    let directory_size = out.len() as u32 - directory_at;

    // 中央目录结束记录
    put_u32(&mut out, 0x0605_4b50);
    put_u16(&mut out, 0); // 本磁盘号
    put_u16(&mut out, 0); // 中央目录所在磁盘号
    put_u16(&mut out, entries.len() as u16);
    put_u16(&mut out, entries.len() as u16);
    put_u32(&mut out, directory_size);
    put_u32(&mut out, directory_at);
    put_u16(&mut out, 0); // 注释长度
    out
}

/// ZIP 用的 CRC-32（IEEE 802.3，反射，初值/终值与标准一致）。
///
/// 逐位算，不建表：投稿包在几 MB 量级，这点开销换一份"一眼能看懂"的实现很值。
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准的 CRC-32 自检值：算错这一条，整包都会被解压工具拒收。
    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_package_starts_with_a_local_header_and_ends_with_a_directory_record() {
        let files = [Entry { name: "a.txt", data: b"hello" }, Entry { name: "b/c.xml", data: b"<x/>" }];
        let zip = write_stored(&files);
        assert_eq!(&zip[0..4], b"PK\x03\x04", "要以局部文件头开头");
        assert_eq!(&zip[zip.len() - 22..zip.len() - 18], b"PK\x05\x06", "要以中央目录结束记录收尾");
        // 两条目录项 + 两条局部头
        assert_eq!(zip.windows(4).filter(|w| *w == b"PK\x01\x02").count(), 2);
        assert_eq!(zip.windows(4).filter(|w| *w == b"PK\x03\x04").count(), 2);
        // STORE：文件内容原样出现在包里
        assert!(zip.windows(5).any(|w| w == b"hello"));
    }

    #[test]
    fn the_same_input_gives_the_same_bytes() {
        let files = [Entry { name: "one.xml", data: b"<a>1</a>" }];
        assert_eq!(write_stored(&files), write_stored(&files));
    }
}
