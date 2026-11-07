//! 把「上次的窗口落点」接到真的窗口上：启动时摆回去、关窗前记下来。
//!
//! 两件事都**不许拦住启动或退出**：
//!
//! - 恢复失败（没有记录、那块屏没了、系统不让摆）→ 保持窗口配置里的居中，什么都不说；
//! - 记录失败（库忙、盘满）→ 关窗照旧放行**不能因为"下次不够顺手"多出新理由**。
//!
//! 落点合不合法的判断全在核心（[`yanmo_core::store::WindowPlace::fit`]）：这里只负责
//! 问窗口要事实（位置 / 尺寸 / 显示器可用区），并守住"**只记普通状态**"这一条——
//! 最小化时 Windows 给的是 `-32000` 那一类哨兵值，最大化记下来会让下次打开占满整块屏，
//! 两种都不是作者亲手摆出来的样子。

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};
use yanmo_core::store::{WindowPlace, WorkArea};

use crate::storage::AppData;

/// 启动时（窗口还没显示）按记录摆好；没有记录、屏幕变了、摆不动都保持原状。
pub fn restore(data: &AppData, window: &WebviewWindow) {
    // 只读地看一眼：读失败当没记过（启动不能为这件事停下来）
    let Ok(Some(place)) = data.with_store(|store| store.window_place()) else {
        return;
    };
    let Some(place) = place.fit(&work_areas(window)) else {
        return;
    };
    let _ = window.set_position(PhysicalPosition::new(place.x, place.y));
    let _ = window.set_size(PhysicalSize::new(place.width, place.height));
}

/// 关窗前记一笔；记不成也照常关（这是增强，不是闸门）。
pub fn remember(window: &tauri::Window, data: &AppData) {
    let place = place_to_remember(
        window.outer_position().ok(),
        window.inner_size().ok(),
        window.is_minimized().unwrap_or(false),
        window.is_maximized().unwrap_or(false),
    );
    if let Some(place) = place {
        // **不排队、不等锁**：界面可能正握着库锁落盘（甚至正在导出），
        // 关窗这件事绝不能为了记一个位置而站在那儿等。这次记不上就下次，
        // 反正窗口位置只是一份"更顺手"的偏好。
        let _ = data.try_with_store(|store| store.remember_window_place(place));
    }
}

/// 该不该记、记成什么（**纯逻辑**：好把"只记普通状态"这条钉在测试里）。
///
/// 下面任何一种都不记：最小化 / 最大化、位置或尺寸读不到、尺寸是 0。
pub(crate) fn place_to_remember(
    position: Option<PhysicalPosition<i32>>,
    size: Option<PhysicalSize<u32>>,
    minimized: bool,
    maximized: bool,
) -> Option<WindowPlace> {
    if minimized || maximized {
        return None;
    }
    let (position, size) = (position?, size?);
    if size.width == 0 || size.height == 0 {
        return None;
    }
    Some(WindowPlace {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// 每块屏的**可用区**（去掉任务栏之后的那一块），原样交给核心。
fn work_areas(window: &WebviewWindow) -> Vec<WorkArea> {
    window
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|monitor| WorkArea {
            x: monitor.work_area().position.x,
            y: monitor.work_area().position.y,
            width: monitor.work_area().size.width,
            height: monitor.work_area().size.height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, width: u32, height: u32) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
        (PhysicalPosition::new(x, y), PhysicalSize::new(width, height))
    }

    #[test]
    fn a_normal_window_is_remembered_as_it_is() {
        let (position, size) = at(240, 120, 1280, 820);
        assert_eq!(
            place_to_remember(Some(position), Some(size), false, false),
            Some(WindowPlace { x: 240, y: 120, width: 1280, height: 820 })
        );
    }

    #[test]
    fn maximized_and_minimized_are_never_remembered() {
        let (position, size) = at(-32000, -32000, 160, 28);
        assert_eq!(place_to_remember(Some(position), Some(size), true, false), None, "最小化");
        assert_eq!(place_to_remember(Some(position), Some(size), false, true), None, "最大化");
    }

    #[test]
    fn a_window_that_cannot_be_read_or_has_no_size_is_skipped() {
        let (position, size) = at(0, 0, 1280, 820);
        assert_eq!(place_to_remember(None, Some(size), false, false), None);
        assert_eq!(place_to_remember(Some(position), None, false, false), None);
        assert_eq!(place_to_remember(Some(position), Some(PhysicalSize::new(0, 820)), false, false), None);
    }
}
