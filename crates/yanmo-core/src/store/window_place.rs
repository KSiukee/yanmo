//! 窗口落点：**记住上次把窗口放在哪、多大**，下次打开回到原处。
//!
//! # 为什么「兜底」比「记住」更要紧
//!
//! 记录是一份**会过期的事实**：显示器可能被拔了、分辨率可能被调小、远程桌面可能换了一块屏。
//! 照着记录原样放，窗口就可能整个跑到看不见的地方——作者会以为软件坏了（双击没反应）。
//! 所以这里的规矩是：**先看记录还够不够得着某块屏幕，够不着就当没记过**，由调用方回落到居中；
//! 够得着也要把窗口夹回那块屏的可用区里（缩得下就缩），绝不留半个窗口在屏幕外。
//!
//! # 只记普通状态
//!
//! 最大化 / 最小化不写记录（那两种状态的位置与大小不是作者摆出来的样子）：
//! 最小化时 Windows 给的位置是 `-32000` 那一类哨兵值，记下来下次打开就没了；
//! 最大化记下来则会让下次打开变成一块占满屏幕的窗口。判断在壳里做，核心只收普通状态。
//!
//! 记录住在 `settings` 的一个键上（与外观、会话同一张表），**不新建表、不动迁移**。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Store;
use crate::error::Result;
use crate::time::now_millis;

/// 窗口落点在 `settings` 里的键。
const WINDOW_PLACE_KEY: &str = "window.place";

/// 一块屏幕的**可用区**（去掉任务栏之后的那一块）。
///
/// 坐标与尺寸都是**物理像素**，与 [`WindowPlace`] 同一套口径——调用方把系统报的数直接递进来，
/// 这里不做任何单位换算（一处换、一处不换才是错位的来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 窗口上一次的落点：左上角（物理像素）+ 内部尺寸（物理像素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPlace {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 「还看得见」的下限：至少要有这么大一块落在某块屏幕的可用区里。
///
/// 160×80 差不多是一条能抓住的标题栏：比这更小的露出等于看不见，
/// 作者既拖不动它、也不知道那是窗口。这条线**故意比"一个像素也算"宽**。
const MIN_VISIBLE_WIDTH: u32 = 160;
const MIN_VISIBLE_HEIGHT: u32 = 80;

/// 尺寸的上限：再大就不是一块屏幕了（记录被写坏时挡在这里）。
const MAX_SIZE: u32 = 32_768;

impl WindowPlace {
    /// 尺寸说得通吗（宽高非零且不超过上限）。
    ///
    /// 读回来先过这一关：`0×0` 或天文数字都只可能是坏记录，当没记过比照着摆更安全。
    pub fn is_sane(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.width <= MAX_SIZE
            && self.height <= MAX_SIZE
    }

    /// 在这些可用区里求一个**一定看得见**的落点；一块都够不着就返回 `None`。
    ///
    /// 两步：
    /// 1. 只认「露出够多」的屏（交集至少 `MIN_VISIBLE_WIDTH` × `MIN_VISIBLE_HEIGHT`），
    ///    在多块都够得着时选交集面积最大的一块；
    /// 2. 把窗口**整个夹进**那块屏：先缩到不超过可用区，再把左上角夹进范围内。
    ///
    /// 于是「屏幕变小了」与「记录比屏幕大」都由第 2 步收口，不会出现窗口比屏幕还大、
    /// 右边和下边永远点不到的局面。
    pub fn fit(self, areas: &[WorkArea]) -> Option<Self> {
        let mut best: Option<(u64, WorkArea)> = None;
        for area in areas {
            let left = i64::from(self.x).max(i64::from(area.x));
            let top = i64::from(self.y).max(i64::from(area.y));
            let right = (i64::from(self.x) + i64::from(self.width))
                .min(i64::from(area.x) + i64::from(area.width));
            let bottom = (i64::from(self.y) + i64::from(self.height))
                .min(i64::from(area.y) + i64::from(area.height));
            let visible_width = (right - left).max(0) as u64;
            let visible_height = (bottom - top).max(0) as u64;
            if visible_width < u64::from(MIN_VISIBLE_WIDTH)
                || visible_height < u64::from(MIN_VISIBLE_HEIGHT)
            {
                continue;
            }
            let overlap = visible_width * visible_height;
            if best.map_or(true, |(seen, _)| overlap > seen) {
                best = Some((overlap, *area));
            }
        }

        let (_, area) = best?;
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        // 夹取的上界是「窗口右下角贴着可用区右下角」那个位置。
        // `width <= area.width`（上面刚取过 min），所以上界不会小于下界。
        let max_x = i64::from(area.x) + i64::from(area.width) - i64::from(width);
        let max_y = i64::from(area.y) + i64::from(area.height) - i64::from(height);
        Some(Self {
            x: i64::from(self.x).clamp(i64::from(area.x), max_x) as i32,
            y: i64::from(self.y).clamp(i64::from(area.y), max_y) as i32,
            width,
            height,
        })
    }
}

/// 读记录：没记过、写坏了、尺寸说不通——一律当没记过（回落到居中）。
fn read_place(conn: &Connection) -> Result<Option<WindowPlace>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![WINDOW_PLACE_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw
        .and_then(|text| serde_json::from_str::<WindowPlace>(&text).ok())
        .filter(WindowPlace::is_sane))
}

impl Store {
    /// 上次的窗口落点（没记过 / 坏记录 → `None`）。
    ///
    /// **只读**：启动时看一眼不该留下任何痕迹。
    pub fn window_place(&self) -> Result<Option<WindowPlace>> {
        read_place(&self.conn)
    }

    /// 记下这次关窗时的落点。
    ///
    /// 写失败不算错事，但**调用方不许因为写失败而拦住关窗**：这只是让下次更顺手，
    /// 不能变成"窗口关不掉"的新理由（壳那边按「忽略错误」处理）。
    pub fn remember_window_place(&self, place: WindowPlace) -> Result<()> {
        // 先自己过一遍尺寸关：坏值绝不落库（落进去下次还得靠读侧再挡一次）。
        if !place.is_sane() {
            return Ok(());
        }
        let json = serde_json::to_string(&place).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)",
            params![WINDOW_PLACE_KEY, json, now_millis()],
        )?;
        Ok(())
    }
}
