//! 命令行入口（薄壳）：把参数交给 [`yanmo_cli::run`]，用它的返回值当退出码。

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(yanmo_cli::run(argv));
}
