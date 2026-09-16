//! 设定卡命令域：**人物 / 设定**的读与写（大纲冲突检测的数据源之一）。
//!
//! 与碎片、问题卡的分工：这里管的是**实体**（有身份、有键值属性的设定），
//! 碎片管的是素材。规则在核心的 `outline`（只报告、不自动改稿），这里只做参数转换。
//!
//! 两个口径（与核心一致）：
//!
//! - **整卡覆盖**：界面上一个表单全摆着，改完就整份交上来（不做稀疏补丁）；
//! - **空项入口就丢**：别称与整条空的属性是手滑，丢掉；只有键没值的那条**留着**
//!   （"还没填"是作者的状态）。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::ApiError;
use crate::storage::AppData;
use yanmo_core::model::{Attribute, EntityCard, EntityKind, NewEntityCard};

/// 界面交上来的一份卡：`kind` 与 `name` 必填，其余可以空着。
#[derive(Debug, Deserialize)]
pub struct EntityCardForm {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub attributes: Vec<Attribute>,
    #[serde(default)]
    pub note: String,
}

/// 一整屏：这本书的设定卡 + **按类型分的数字**（面板上那两个筛选项）。
#[derive(Debug, Serialize)]
pub struct EntityBoardDto {
    pub cards: Vec<EntityCard>,
    pub persons: usize,
    pub settings: usize,
}

fn board(store: &yanmo_core::store::Store, work_id: i64) -> yanmo_core::Result<EntityBoardDto> {
    let cards = store.entity_cards(work_id, None)?;
    let persons = cards.iter().filter(|card| card.kind == EntityKind::Person).count();
    let settings = cards.len() - persons;
    Ok(EntityBoardDto { cards, persons, settings })
}

/// 把界面那份转成核心要的形状（**类型码在这一处解析**：认不出当场拒）。
fn to_new(form: &EntityCardForm, work_id: i64) -> yanmo_core::Result<NewEntityCard> {
    Ok(NewEntityCard {
        work_id,
        kind: EntityKind::parse(&form.kind)?,
        name: form.name.clone(),
        aliases: form.aliases.clone(),
        attributes: form.attributes.clone(),
        note: form.note.clone(),
    })
}

/// 列这本书的设定卡（人物 / 设定一起给，按名字排）。
#[tauri::command(rename_all = "snake_case")]
pub fn entity_list(data: State<'_, AppData>, work_id: i64) -> Result<EntityBoardDto, ApiError> {
    crate::acceptance::note_command("entity_list");
    data.with_store(|store| board(store, work_id))
}

/// 新建一张（名字空着会被核心当场拒）。
#[tauri::command(rename_all = "snake_case")]
pub fn entity_create(
    data: State<'_, AppData>,
    work_id: i64,
    form: EntityCardForm,
) -> Result<EntityBoardDto, ApiError> {
    crate::acceptance::note_command("entity_create");
    data.with_store(|store| {
        let draft = to_new(&form, work_id)?;
        store.create_entity_card(&draft, "author")?;
        board(store, work_id)
    })
}

/// 改一张（**整卡覆盖**：表单里是什么样，改完就是什么样）。
#[tauri::command(rename_all = "snake_case")]
pub fn entity_update(
    data: State<'_, AppData>,
    id: i64,
    form: EntityCardForm,
) -> Result<EntityBoardDto, ApiError> {
    crate::acceptance::note_command("entity_update");
    data.with_store(|store| {
        // 这本书是哪一本由卡自己说了算（界面不必再报一遍，也就不可能报错一本）
        let work_id = store.entity_card(id)?.work_id;
        let draft = to_new(&form, work_id)?;
        store.update_entity_card(id, &draft, "author")?;
        board(store, work_id)
    })
}

/// 删一张（**软删**：库里还留着；这一版界面上不留回头路，数据留着以后好办）。
#[tauri::command(rename_all = "snake_case")]
pub fn entity_delete(data: State<'_, AppData>, id: i64) -> Result<EntityBoardDto, ApiError> {
    crate::acceptance::note_command("entity_delete");
    data.with_store(|store| {
        let gone = store.delete_entity_card(id, "author")?;
        board(store, gone.work_id)
    })
}
