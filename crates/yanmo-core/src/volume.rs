//! 分卷的**规则**：一卷大概几章、什么时候该劝作者收卷。不碰数据库，也不碰界面。
//!
//! 口径只写在这一处（存储层与界面都来取），三条：
//!
//! 1. **阈值跟他自己比**：作者设过「大概几章一卷」就用它；一旦这本书已经有**两个以上
//!    收好的卷**，改用**他实际收卷点的中位数**——越写越像他本人的节奏
//!    （别拿一把通用尺子量他：他自己收在几章，尺子就该是几章）；
//! 2. **窗口**：**80% 起就可以收**（提前），**120% 之后话术加重但不停口**
//!    （延后——若在 120% 直接闭嘴，他反而彻底没有入口了，只能自己算号建卷）；
//! 3. **只提议、不动结构**：这里只回答"要不要在这一章后面提一句"，收不收由他按
//!    （本地规则会误判，系统没资格替他决定卷边界）。
//!
//! 本模块是纯函数：给数字进去、给结论出来，所以阈值可以脱离数据库单测。

/// 这一卷已经写到的位置，相对阈值分三档（界面据它换话术）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumePhase {
    /// 还没到阈值（80% ~ 100%）：收在这一章属于**提前**收卷
    Early,
    /// 正好到阈值
    OnTarget,
    /// 超过阈值（> 120% 就是"超得不少了"）
    Late,
}

impl VolumePhase {
    /// 稳定代码（进界面；别改）。
    pub const fn as_str(self) -> &'static str {
        match self {
            VolumePhase::Early => "early",
            VolumePhase::OnTarget => "on_target",
            VolumePhase::Late => "late",
        }
    }
}

/// 一本书的分卷口径（界面拿 `effective` 显示「本卷 12/30 章」的分母）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VolumePlan {
    /// 作者自己定的（没设过是 `None`——不猜默认值）
    pub target: Option<i64>,
    /// 从他**收好的卷**学到的中位数（样本 ≥2 才有；一个样本取中位数没有意义）
    pub learned: Option<i64>,
    /// 真正在用的阈值：学到的优先，其次作者定的；两个都没有 = 没得提示
    pub effective: Option<i64>,
}

/// 收卷提议：**在这一章之后**可以收卷了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeOffer {
    pub plan: VolumePlan,
    /// 到这一章为止，这一卷（根层就是"上一卷之后的这一段"）已有几章
    pub count: i64,
    pub phase: VolumePhase,
}

/// 一处收卷点：提议本身 + **它落在哪一卷上**。
///
/// 为什么要带上"哪一卷"：界面要按卷记住"这一卷已经问过一次了"（连着两次「再等等」
/// 就不再提），键得有一个稳定的东西可用——收卷点所在的卷就是它（散在根上时是 `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeSpot {
    /// 收卷点落在哪一卷里；`None` = 章还散在根上（还没进过卷）
    pub container: Option<i64>,
    pub offer: VolumeOffer,
}

/// 起提线：到阈值的 80% 就可以收了（提前收卷是正常创作决定）。
pub const EARLY_PERCENT: i64 = 80;
/// 加重线：过了 120% 话术变重（但**不停口**，见文件头第 2 条）。
pub const LATE_PERCENT: i64 = 120;

/// 中位数：奇数个取正中；偶数个取中间两个的平均（四舍五入）。
///
/// 空列表是 `None`（没有样本 ≠ 样本是 0）。
pub fn median(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[middle])
    } else {
        Some((sorted[middle - 1] + sorted[middle] + 1) / 2)
    }
}

/// 真正在用的阈值：**收好的卷 ≥2 个就听他的历史**，否则听他自己设的那个数。
///
/// `history` 是每一卷的章数（只算收好的那些卷——正在写的这一卷还没收，不算样本）。
pub fn effective_target(target: Option<i64>, history: &[i64]) -> VolumePlan {
    let learned = if history.len() >= 2 { median(history) } else { None };
    let effective = learned.or(target).filter(|n| *n > 0);
    VolumePlan { target: target.filter(|n| *n > 0), learned, effective }
}

/// 起提线 / 加重线（按阈值算，向上取整：7 章的 80% 是 5.6，从第 6 章起就该提）。
pub fn window(effective: i64) -> (i64, i64) {
    let percent = |p: i64| (effective * p + 99) / 100;
    (percent(EARLY_PERCENT), percent(LATE_PERCENT))
}

/// 该不该在"这一卷到第 `count` 章"时说一句收卷。
///
/// 没阈值（既没设过、也没有历史）一律 `None`——**不写死一个默认值**，
/// 免得给所有作者安上同一把尺子。
pub fn offer(plan: VolumePlan, count: i64) -> Option<VolumeOffer> {
    let effective = plan.effective?;
    let (low, _) = window(effective);
    if count < low {
        return None;
    }
    let phase = if count < effective {
        VolumePhase::Early
    } else if count == effective {
        VolumePhase::OnTarget
    } else {
        VolumePhase::Late
    };
    Some(VolumeOffer { plan, count, phase })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_takes_the_middle_and_rounds_even_samples() {
        assert_eq!(median(&[]), None, "没有样本 ≠ 样本是 0");
        assert_eq!(median(&[30]), Some(30));
        assert_eq!(median(&[30, 28, 26]), Some(28));
        assert_eq!(median(&[24, 30]), Some(27), "偶数个取中间两个的平均");
        assert_eq!(median(&[25, 30]), Some(28), "平均出 .5 时四舍五入");
        assert_eq!(median(&[40, 12, 30]), Some(30), "顺序乱着进来也要对");
    }

    #[test]
    fn two_finished_volumes_are_enough_to_learn_from() {
        // 只收好一卷：一个样本不当中位数，听作者设的
        let one = effective_target(Some(30), &[12]);
        assert_eq!(one.learned, None);
        assert_eq!(one.effective, Some(30));
        // 收好两卷：改听历史（作者设的 30 让位给他实际的节奏）
        let two = effective_target(Some(30), &[22, 24]);
        assert_eq!(two.learned, Some(23));
        assert_eq!(two.effective, Some(23));
        assert_eq!(two.target, Some(30), "他设的那个数还留着，好在界面上说清来历");
    }

    #[test]
    fn no_target_and_no_history_means_no_offer() {
        let plan = effective_target(None, &[]);
        assert_eq!(plan.effective, None);
        assert_eq!(offer(plan, 30), None, "没尺子就别量人家");
        // 作者清了目标（≤0 当没设过）、历史又不够两卷：退回"不提示"
        assert_eq!(effective_target(Some(0), &[9]).effective, None);
    }

    #[test]
    fn window_is_eighty_to_one_twenty_percent_rounded_up() {
        assert_eq!(window(30), (24, 36));
        assert_eq!(window(7), (6, 9), "5.6 与 8.4 都往上收");
        assert_eq!(window(100), (80, 120));
    }

    #[test]
    fn offer_starts_at_eighty_percent_and_never_shuts_up() {
        let plan = effective_target(Some(30), &[]);
        assert_eq!(offer(plan, 23), None, "80% 之前不打扰");
        assert_eq!(offer(plan, 24).unwrap().phase, VolumePhase::Early, "80% 起就是提前收卷");
        assert_eq!(offer(plan, 30).unwrap().phase, VolumePhase::OnTarget);
        assert_eq!(offer(plan, 31).unwrap().phase, VolumePhase::Late);
        // 过了 120% 话术加重，但**仍然给入口**：闭嘴他就只能自己算号建卷了
        assert_eq!(offer(plan, 36).unwrap().phase, VolumePhase::Late);
        assert_eq!(offer(plan, 99).unwrap().phase, VolumePhase::Late);
        assert_eq!(offer(plan, 99).unwrap().count, 99);
    }

    #[test]
    fn learned_plan_drives_the_offer_not_the_stale_target() {
        // 他设的是 30，实际两卷收在 22 / 24：阈值按 23 算（80% = 19 章就该提）
        let plan = effective_target(Some(30), &[22, 24]);
        assert_eq!(offer(plan, 18), None);
        assert_eq!(offer(plan, 19).unwrap().phase, VolumePhase::Early);
        assert_eq!(offer(plan, 19).unwrap().plan.effective, Some(23));
    }
}
