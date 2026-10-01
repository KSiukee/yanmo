//! 「正文里冒出来的名字」验收：**认得出该认的、认不出就不吭声**。
//!
//! 这一条与其余六条规则的分寸不一样：它产出的是**候选**（"这个名字我认不出来"），
//! 所以这里盯的是**假警报**那一侧——一个都不许多报。
//! 判据全在纯逻辑那一层（喂值、看发现），库那一半只验"接线接对了"。

use yanmo_core::model::{EntityCard, EntityKind, NewEntityCard, NodeKind, WorkKind};
use yanmo_core::outline::{unlisted_name_issues, unlisted_names, IssueRule, UNLISTED_MAX};
use yanmo_core::store::Store;

fn card(name: &str, aliases: &[&str]) -> EntityCard {
    EntityCard {
        id: 1,
        work_id: 1,
        kind: EntityKind::Person,
        name: name.to_string(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        attributes: Vec::new(),
        note: String::new(),
        created_at: 0,
        updated_at: 0,
    }
}

/// 抽一遍（不碰库）：正文按章给。
fn names(bodies: &[&str], cards: &[EntityCard]) -> Vec<String> {
    let chapters: Vec<(i64, &str)> = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| (index as i64 + 1, *body))
        .collect();
    unlisted_names(&chapters, cards)
        .into_iter()
        .map(|name| format!("{}:{}:{}", name.form, name.kind.as_str(), name.count))
        .collect()
}

#[test]
fn a_title_frame_finds_the_name_in_front_of_it() {
    // 「李大人」出现两次 ⇒ 够门槛
    let found = names(&["李大人负手而立。李大人看了他一眼。"], &[]);
    assert_eq!(found, vec!["李大人:person:2"]);
}

#[test]
fn a_surname_frame_needs_the_speech_or_action_word() {
    // 紧跟着说话动作词 ⇒ 认得到；只是被人喊了一声（后面是标点）⇒ 认不到
    assert_eq!(names(&["张三说道：走。张三笑了笑。"], &[]), vec!["张三:person:2"]);
    assert_eq!(names(&["从前有个人，姓张名三。"], &[]), Vec::<String>::new());
}

#[test]
fn a_place_needs_its_generic_suffix_and_length() {
    // 通名那一路：出现三次才算
    assert_eq!(names(&["青城山上有座观。他去了青城山。青城山很高。"], &[]), vec!["青城山:setting:3"]);
    // 只出现一次 ⇒ 连"我认不出"都不说
    assert_eq!(names(&["他去了青城山。"], &[]), Vec::<String>::new());
}

#[test]
fn the_start_of_a_name_must_be_a_break() {
    // 「走出青城山」不许切出「出青城山」这种东西——起点必须碰得上断点
    let found = names(&["走出青城山。又走出青城山。还是走出青城山。"], &[]);
    assert_eq!(found, vec!["青城山:setting:3"], "只许报真名，不许报切出来的那截");
    // 「这位大人」左边是量词 ⇒ 那不是"名字+称谓"（"大人"自己不是名字）
    assert_eq!(names(&["这位大人说了两次，那位大人也说了。"], &[]), Vec::<String>::new());
    // 「大师兄」是叫法不是名字：前缀就一个修饰字 ⇒ 丢掉
    assert_eq!(names(&["大师兄摇头。大师兄又摇头。"], &[]), Vec::<String>::new());
}

#[test]
fn a_start_we_cannot_find_is_not_guessed() {
    // 通名前面连着四个"名字外"的字（尽 头 青 城）：往前找三个字还没碰上一个断点，
    // 说明**起点认不出来**——那就别猜（猜下去会切出"青头尽城"这种东西）。
    // 这一条钉的是 `prefix_start` 里那道"走满三个字还没碰上断点就放弃"的闸。
    let text = "尽头青城山。又尽头青城山。还是尽头青城山。";
    assert_eq!(names(&[text], &[]), Vec::<String>::new(), "起点认不出来时一个字都不许报");
}

#[test]
fn what_is_already_on_a_card_is_not_reported() {
    let cards = [card("李大人", &[])];
    assert_eq!(names(&["李大人说了。李大人又说。"], &cards), Vec::<String>::new());
    // 设定卡叫「青城」，正文写「青城山」——那是同一处地方（双向包含）
    let place = EntityCard { kind: EntityKind::Setting, ..card("青城", &[]) };
    assert_eq!(names(&["青城山上。去了青城山。青城山高。"], &[place]), Vec::<String>::new());
    // 别称也算登记过的叫法
    let aliased = [card("陆文", &["陆大人"])];
    assert_eq!(names(&["陆大人点头。陆大人又说。"], &aliased), Vec::<String>::new());
}

#[test]
fn the_signals_do_not_fire_on_ordinary_prose() {
    let prose = "他走进大殿，抬头看了看。这座山很高，那座山也不矮。草原一望无际，森林密布。\
                 张三和李四一同进去，王五跟在后面。他们并不知道前面发生了什么。";
    assert_eq!(names(&[prose], &[]), Vec::<String>::new(), "普通叙述里一个假名字都不许冒出来");
}

#[test]
fn issues_carry_the_code_and_cap_honestly() {
    // 造 45 条够格的候选，看上限与"截了几条"的如实报数
    let given = [
        "赵刚", "钱明", "孙武", "李青", "周正", "吴用", "郑和", "陈平", "卫青", "蒋干", "沈括",
        "韩非", "杨戬", "朱亥", "秦琼", "许昌", "何晏", "吕雉", "施公", "张辽", "孔融", "曹参",
        "严光", "华佗", "金庸", "魏征", "陶潜", "姜维", "谢安", "邹忌", "柏舟", "窦融", "章邯",
        "云长", "苏武", "潘安", "葛洪", "奚仲", "范蠡", "彭越", "宋玉", "唐寅", "韩愈", "冯唐",
        "邓通",
    ];
    assert_eq!(given.len(), 45);
    let body: String = given
        .iter()
        .map(|name| format!("{name}大人说了。{name}大人又说。"))
        .collect();
    let all = unlisted_names(&[(1, body.as_str())], &[]);
    assert_eq!(all.len(), 45);
    let (issues, hidden) = unlisted_name_issues(&all);
    assert_eq!(issues.len(), UNLISTED_MAX);
    assert_eq!(hidden, 45 - UNLISTED_MAX, "截掉的条数要如实报");
    assert_eq!(issues[0].rule, IssueRule::EntityUnlistedName);
    assert_eq!(issues[0].rule.as_str(), "entity.unlisted_name");
    let params = &issues[0].params;
    assert_eq!(params.get("count").map(String::as_str), Some("2"));
    assert!(params.contains_key("node_id"), "定位走 node_id（锚点留空，见核心注释）");
    // 身份就是这个写法本身：忽略一次，下一轮还是同一条
    let form = params["form"].clone();
    assert_eq!(issues[0].fingerprint(), format!("entity.unlisted_name||{form}"));
    // 认不出的码不当规则（界面把忽略读回来时要认它）
    assert_eq!(IssueRule::parse("entity.unlisted_name"), Some(IssueRule::EntityUnlistedName));
}

fn fresh() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("yanmo.db")).unwrap();
    (dir, store)
}

#[test]
fn the_store_scans_every_chapter_and_skips_what_is_on_a_card() {
    let (_dir, mut store) = fresh();
    let work = store.create_work(WorkKind::Novel, "长夜").unwrap();
    let volume = store.list_nodes(work.id).unwrap()[0].id;
    let mut chapters = Vec::new();
    for title in ["第一章", "第二章", "第三章"] {
        chapters.push(store.create_node(work.id, Some(volume), NodeKind::Chapter, title).unwrap());
    }
    store
        .create_entity_card(
            &NewEntityCard {
                work_id: work.id,
                kind: EntityKind::Person,
                name: "陆文".to_string(),
                aliases: vec!["陆大人".to_string()],
                attributes: Vec::new(),
                note: String::new(),
            },
            "test",
        )
        .unwrap();
    store.write_body(chapters[0], "李大人说了什么。").unwrap();
    store.write_body(chapters[1], "李大人又说了什么。").unwrap();
    store.write_body(chapters[2], "陆大人一句话都没说。").unwrap();

    let report = store.unlisted_names(work.id).unwrap();
    assert_eq!(report.scanned, 3, "正文单位三章都该扫到");
    assert_eq!(report.hidden, 0);
    let forms: Vec<&str> = report
        .issues
        .iter()
        .filter_map(|issue| issue.params.get("form").map(String::as_str))
        .collect();
    assert_eq!(forms, vec!["李大人"], "设定卡上已有的人不许再报");
    let first = report.issues[0].params.get("node_id").unwrap();
    assert_eq!(first, &chapters[0].to_string(), "定位到它第一次出现的那一章");
}
