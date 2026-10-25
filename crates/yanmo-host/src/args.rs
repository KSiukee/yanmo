//! 命令行参数小工具：只做 `--flag value` 的读取，不引入 clap。

pub fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

pub fn number_of(args: &[String], flag: &str) -> Option<u64> {
    value_of(args, flag).and_then(|value| value.parse().ok())
}
