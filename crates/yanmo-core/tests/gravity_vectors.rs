//! 跨语言对齐向量：**这次 port 与源头算得一模一样**。
//!
//! 机制是复制过来的（见 `gravity` 各文件头的血统说明），但"复制得对不对"不能靠读代码印象——
//! 所以拿源头仓库的**测试向量**逐例核对：同一个输入、同一条公式，结果必须落在 1e-9 之内。
//! 三端（源头两种语言 + 本项目）各实现一遍机制时，这是唯一可靠的对齐办法。
//!
//! 向量文件只保留了本项目**真的复制了**的那四个函数的用例（数字一个未改）；
//! 另外几个函数的用例属于没复制的模块，留着只会让这个测试变成摆设。

use std::collections::BTreeSet;

use serde_json::Value;
use yanmo_core::gravity::{
    decayed_strength, decide_level, reinforce, should_sink, ForgettingParams, FragmentLevel,
    StateMachineParams,
};

/// 对齐向量（随测试一起进仓：脱离了源仓也能跑）。
const VECTORS: &str = include_str!("vectors/gravity-vectors.json");

/// 向量里的档位字符串 → 本项目的档位。
fn level_of(code: &str) -> FragmentLevel {
    match code {
        "short" => FragmentLevel::Short,
        "mid" => FragmentLevel::Mid,
        "long" => FragmentLevel::Long,
        other => panic!("向量里出现了我们不认识的档位：{other}"),
    }
}

/// 曲线参数：从向量给的取值起，其余用默认（默认值本身也要与源头一致，下面另有一条测试钉）。
fn forgetting_params(params: &Value) -> ForgettingParams {
    let base = ForgettingParams::default();
    ForgettingParams {
        decay_rate: params["decay_rate"].as_f64().unwrap_or(base.decay_rate),
        reinforcement_delta: params["reinforcement_delta"]
            .as_f64()
            .unwrap_or(base.reinforcement_delta),
        strength_floor: params["strength_floor"].as_f64().unwrap_or(base.strength_floor),
        level_thresholds: base.level_thresholds,
    }
}

/// 档位参数：同理。
fn level_params(params: &Value) -> StateMachineParams {
    let base = StateMachineParams::default();
    StateMachineParams {
        mid_term_threshold: params["mid_term_threshold"]
            .as_u64()
            .map_or(base.mid_term_threshold, |v| v as usize),
        long_term_threshold: params["long_term_threshold"]
            .as_u64()
            .map_or(base.long_term_threshold, |v| v as usize),
        importance_threshold: params["importance_threshold"]
            .as_f64()
            .unwrap_or(base.importance_threshold),
        short_term_retention_days: base.short_term_retention_days,
        mid_term_retention_days: base.mid_term_retention_days,
    }
}

#[test]
fn the_port_reproduces_every_reference_vector() {
    let doc: Value = serde_json::from_str(VECTORS).expect("向量文件要能解析");
    let tolerance = doc["tolerance"].as_f64().unwrap_or(1e-9);
    let cases = doc["cases"].as_array().expect("向量文件里要有 cases");
    let mut functions = BTreeSet::new();
    let mut checked = 0usize;

    for case in cases {
        let name = case["fn"].as_str().expect("每个用例都要写清是哪个函数");
        let args = case["args"].as_array().expect("每个用例都要有参数");
        let expected = &case["expected"];
        functions.insert(name.to_string());
        checked += 1;

        match name {
            "decayed_strength" => {
                let p = forgetting_params(&case["params"]);
                let got = decayed_strength(args[0].as_f64().unwrap(), args[1].as_f64().unwrap(), &p);
                let want = expected.as_f64().unwrap();
                assert!(
                    (got - want).abs() <= tolerance,
                    "decayed_strength{args:?}：算出 {got}，向量要 {want}"
                );
            }
            "reinforce" => {
                let p = forgetting_params(&case["params"]);
                let got = reinforce(args[0].as_f64().unwrap(), &p);
                let want = expected.as_f64().unwrap();
                assert!(
                    (got - want).abs() <= tolerance,
                    "reinforce{args:?}：算出 {got}，向量要 {want}"
                );
            }
            "should_sink" => {
                let p = forgetting_params(&case["params"]);
                let got = should_sink(args[0].as_f64().unwrap(), level_of(args[1].as_str().unwrap()), &p);
                assert_eq!(got, expected.as_bool().unwrap(), "should_sink{args:?} 判定不一致");
            }
            "decide_level" => {
                let p = level_params(&case["params"]);
                let got = decide_level(args[0].as_u64().unwrap() as usize, args[1].as_f64().unwrap(), &p);
                // 比字符串而不是比枚举：向量里的值就是存储与跨语言交换用的那一种写法
                assert_eq!(
                    serde_json::to_value(got).unwrap(),
                    *expected,
                    "decide_level{args:?} 分档不一致"
                );
            }
            other => panic!("向量文件里混进了没复制的函数：{other}"),
        }
    }

    // 防"测试假绿"：核对过的用例数与函数集合都要对得上，不许因为过滤写错而一条不跑
    assert!(checked >= 14, "只核对了 {checked} 例——向量文件是不是被过滤坏了？");
    assert_eq!(
        functions,
        BTreeSet::from([
            "decayed_strength".to_string(),
            "reinforce".to_string(),
            "should_sink".to_string(),
            "decide_level".to_string(),
        ])
    );
}

/// 默认参数也要与源头一致：公式对、旋钮不对，等于换了条曲线还说"一样"。
#[test]
fn default_params_match_the_reference() {
    let doc: Value = serde_json::from_str(VECTORS).unwrap();
    let cases = doc["cases"].as_array().unwrap();

    let curve = cases
        .iter()
        .find(|c| c["fn"] == "decayed_strength")
        .expect("向量里要有曲线用例");
    let params = forgetting_params(&curve["params"]);
    assert_eq!(params, ForgettingParams::default(), "曲线参数与源头不一致");

    let levels = cases
        .iter()
        .find(|c| c["fn"] == "decide_level")
        .expect("向量里要有分档用例");
    assert_eq!(
        level_params(&levels["params"]),
        StateMachineParams::default(),
        "分档参数与源头不一致"
    );
}
