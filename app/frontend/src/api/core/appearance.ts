// 外观偏好（全局 / 每书覆盖）。

import { call, COMMANDS } from "./transport";

/** 外观 / 写作行为偏好（每项都已经落到具体值）。 */
export interface Appearance {
  /** 打开最新一章时跳到段末并聚焦输入光标 */
  jump_to_end_on_latest: boolean;
  /** 作者选过的字数口径（chars / chars_no_punct / words）；null = 没选过，跟作品语言走 */
  word_count_caliber: string | null;
  /** 引号用哪一套（curly / corner）：排版清理按它统一引号 */
  quote_style: string;
  /** 每日码字目标（跟状态栏当前口径走）；null = 没设目标 */
  daily_goal: number | null;
  /** 没选过（null）= 按作品类型：长篇给号、单篇与文集不编号 */
  naming: string | null;
  /** 章的号跨不跨卷数（continue / per_volume）；默认跨卷延续（continue） */
  chapter_numbering: string;
  /** 正文排版三项（**只影响观感，不进导出**）；null = 没改过，界面用自己那档默认 */
  editor_font_size: number | null;
  editor_line_height: number | null;
  editor_letter_spacing: number | null;
  /** 正文字体：五档预设码（serif/sans/kai/yahei/dengxian）或 `family:<字体名>` / `file:<文件名>` */
  editor_font: string;
  /** 字体生效范围：body = 仅正文 / all = 整个界面 */
  editor_font_scope: string;
  /** 正文底下那层稿纸横线用哪种线（none / solid / dashed）；**只影响观感，不进导出** */
  editor_rule_style: string;
  /** 叩问问话的语气（warm / neutral / direct）；neutral 就是"不加语气"＝把这层关掉 */
  question_tone: string;
  /** 写的时候最多主动问几次（0 = 不打扰）；**只限"推"**，面板随时能开 */
  question_push_per_day: number;
  /** 两次主动问之间的冷却（分钟） */
  question_push_cooldown_minutes: number;
  /** 右侧第二栏先露哪一块（`flow` / `creator`）——"记住上次"的就是它 */
  aside_tab: string;
  /** 界面配色（light / dark / system）；**只走全局**这一层 */
  theme: string;
  /** 用眼提醒：开着没有（**只走全局**；关掉就是一声不响） */
  break_reminder: boolean;
  /** 连着写满多少分钟提醒一次（默认 45；上限 180） */
  break_minutes: number;
  /** 今天写到多少字提醒一次（默认 1500；上限 20000） */
  break_words: number;
  /** 护眼滤色的幅度（0~100；**0 = 关**，一个像素都不染）；**只走全局**这一层 */
  eye_care: number;
  /** 正文底色（paper / cream / green / gray）；**只影响观感，不进导出** */
  editor_bg_color: string;
  /** 正文底纹（none / grid / dots / grain）；一格多大由界面按字号 × 行距算 */
  editor_bg_layout: string;
  /** 背景图**在数据目录里的文件名**；null = 没有图（**永远不给路径**） */
  editor_bg_image: string | null;
  /** 背景图铺法（cover / contain / tile） */
  editor_bg_fit: string;
  /** 背景图那一层的不透明度（0~100） */
  editor_bg_opacity: number;
}

/** 要改的偏好项：**只写传进来的**，没传的保持原样。 */
export interface AppearancePatch {
  jump_to_end_on_latest?: boolean;
  word_count_caliber?: string;
  quote_style?: string;
  /** 传 0 就是**清掉目标**，传正数就是设成它 */
  daily_goal?: number;
  /** arabic / chinese / padded / none；传 "auto" 就是**清掉这一层**（回到按作品类型） */
  naming?: string;
  /** continue / per_volume；传 "auto" 就是**清掉这一层**（回到默认：跨卷延续） */
  chapter_numbering?: string;
  /** 正文排版：字号 px / 行距百分比 / 字距百分比；**0 = 清掉这一项**（回默认档） */
  editor_font_size?: number;
  editor_line_height?: number;
  editor_letter_spacing?: number;
  /** 正文字体：预设码或 `family:<字体名>` / `file:<文件名>`；传 "auto" 就是**清掉这一层**（回默认宋体那档） */
  editor_font?: string;
  /** 字体生效范围：body / all；传 "auto" 就是**清掉这一层**（回默认仅正文） */
  editor_font_scope?: string;
  /** 稿纸线型：none / solid / dashed；传 "auto" 就是**清掉这一层**（回默认：不显示） */
  editor_rule_style?: string;
  /** 正文底色：paper / cream / green / gray；传 "auto" 就是**清掉这一层**（回默认：纸白） */
  editor_bg_color?: string;
  /** 正文底纹：none / grid / dots / grain；传 "auto" 就是**清掉这一层**（回默认：不铺） */
  editor_bg_layout?: string;
  /** 背景图铺法：cover / contain / tile；传 "auto" 就是**清掉这一层**（回默认：铺满） */
  editor_bg_fit?: string;
  /** 背景图那一层的不透明度（0~100，越界由核心夹住）；传负数 = 清掉这一层（回默认 35）。
   *  **0 是正经取值**（图留着但看不见），所以不拿 0 当"清掉"。
   *  图片的**文件名不从这个 patch 走**：它由 `background_pick` 在文件复制成功之后落库。 */
  editor_bg_opacity?: number;
  /** warm / neutral / direct；传 "auto" 就是**清掉这一层**（回默认：温柔） */
  question_tone?: string;
  /** 主动问的次数（0 = 不打扰）与冷却分钟；传负数 = 清掉这一层（回默认） */
  question_push_per_day?: number;
  question_push_cooldown_minutes?: number;
  /** flow / creator；传 "auto" 就是**清掉这一层**（回默认：叩问） */
  aside_tab?: string;
  /** light / dark / system；传 "auto" 就是**清掉这一层**（回默认：浅色）。**只写全局那一层** */
  theme?: string;
  /** 用眼提醒的开关与两个阈值；≤0 或超上限（180 / 20000）会被核心**当场拒**。**只写全局那一层** */
  break_reminder?: boolean;
  break_minutes?: number;
  break_words?: number;
  /** 护眼滤色的幅度（0~100，越界由核心夹住）；传负数 = 清掉这一层（回默认关）。
   *  **0 是正经取值**（＝关），所以不拿 0 当"清掉"。**只写全局那一层** */
  eye_care?: number;
}

/** 读外观 / 写作行为偏好；`work_id` 给 null 就是只看全局那份。 */
export const readAppearance = (work_id: number | null) =>
  call<Appearance>(COMMANDS.appearanceRead, { work_id });

/** 改偏好（稀疏合并），返回**写完回读**的那一份。 */
export const writeAppearance = (work_id: number | null, patch: AppearancePatch) =>
  call<Appearance>(COMMANDS.appearanceWrite, { work_id, patch });

/** 偏好回到默认（书的覆盖则是"回到继承全局"）。 */
export const resetAppearance = (work_id: number | null) =>
  call<Appearance>(COMMANDS.appearanceReset, { work_id });
