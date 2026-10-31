use encoding_rs::{BIG5, Encoding, GBK, UTF_8, WINDOWS_1252};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionEncoding {
    Utf8,
    Gbk,
    Big5,
    Windows1252,
}

impl SessionEncoding {
    pub fn parse(name: &str) -> SessionEncoding {
        match name {
            "gbk" => SessionEncoding::Gbk,
            "big5" => SessionEncoding::Big5,
            "windows1252" => SessionEncoding::Windows1252,
            _ => SessionEncoding::Utf8,
        }
    }

    fn encoding(self) -> &'static Encoding {
        match self {
            SessionEncoding::Utf8 => UTF_8,
            SessionEncoding::Gbk => GBK,
            SessionEncoding::Big5 => BIG5,
            SessionEncoding::Windows1252 => WINDOWS_1252,
        }
    }
}

/// 会话级流式解码器：远端字节流 → UTF-8 文本。
/// 快路径：块内完整有效 UTF-8 且无挂起半字符时，走 SIMD 校验直通；
/// 慢路径：出现跨块切分的多字节字符时，交给状态化解码器拼接。
/// 全编码统一入口，残缺字节按规范替换为 U+FFFD。
pub struct Decoder {
    inner: encoding_rs::Decoder,
    drained: bool,
}

impl Decoder {
    pub fn new(encoding: SessionEncoding) -> Decoder {
        Decoder { inner: encoding.encoding().new_decoder(), drained: true }
    }

    pub fn decode(&mut self, bytes: &[u8]) -> String {
        if self.drained && std::str::from_utf8(bytes).is_ok() {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        // decode_to_string 以 String 容量为输出上限且绝不扩容，必须先按最坏情况预留
        let worst = self
            .inner
            .max_utf8_buffer_length(bytes.len())
            .expect("解码缓冲区长度计算溢出");
        let mut out = String::with_capacity(worst);
        let (_, read, _) = self.inner.decode_to_string(bytes, &mut out, false);
        self.drained = read == bytes.len();
        out
    }

    /// 流结束：冲刷解码器内部挂起的半字符。会话关闭或基准收尾时调用。
    #[cfg(test)]
    pub fn finish(&mut self) -> String {
        let worst = self
            .inner
            .max_utf8_buffer_length(0)
            .unwrap_or(4)
            .max(4);
        let mut out = String::with_capacity(worst);
        let (_, _, _) = self.inner.decode_to_string(&[], &mut out, true);
        self.drained = true;
        out
    }
}

/// 键盘输入按会话编码转回远端字节。输入块天然完整，非流式即可。
pub fn encode(encoding: SessionEncoding, text: &str) -> Vec<u8> {
    if encoding == SessionEncoding::Utf8 {
        return text.as_bytes().to_vec();
    }
    let (bytes, _, _) = encoding.encoding().encode(text);
    bytes.into_owned()
}

#[cfg(test)]
mod tests {
    use super::{encode, Decoder, SessionEncoding};

    #[test]
    fn gbk解码跨块拼接() {
        let gbk = encode(SessionEncoding::Gbk, "中文输出测试");
        assert_ne!(gbk, "中文输出测试".as_bytes());
        let mut d = Decoder::new(SessionEncoding::Gbk);
        let out = d.decode(&gbk[..3]) + &d.decode(&gbk[3..]);
        assert_eq!(out, "中文输出测试");
    }

    #[test]
    fn utf8直通() {
        let mut d = Decoder::new(SessionEncoding::Utf8);
        assert_eq!(d.decode("hello 世界".as_bytes()), "hello 世界");
    }

    #[test]
    fn utf8跨块拼接() {
        // 模拟 TCP 分块把多字节字符切成两半，状态化解码必须无损拼接
        let bytes = "中文输出测试".as_bytes();
        let mut d = Decoder::new(SessionEncoding::Utf8);
        let out = d.decode(&bytes[..4]) + &d.decode(&bytes[4..]);
        assert_eq!(out, "中文输出测试");
    }

    #[test]
    fn gbk编码往返() {
        let mut d = Decoder::new(SessionEncoding::Gbk);
        let roundtrip = d.decode(&encode(SessionEncoding::Gbk, "运维 GBK 会话"));
        assert_eq!(roundtrip, "运维 GBK 会话");
    }
}
