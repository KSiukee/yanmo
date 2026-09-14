<script setup lang="ts">
// 设置面板：**左列分类 + 右列内容**（写作行为 / 新建条目 / 稿子放在哪 / 关于）。
//
// 视图只负责"显示与改"：偏好的真相在核心（单一真相源）；读不出来就如实说，不猜一个默认值糊上去。
// 「稿子放在哪」只显示壳报告出来的路径，另外把"换位置"那个面板请出来（它自己那套分寸见组件里）。
// 「关于」是只读报告：版本 / 库结构 + 一键打开稿子文件夹 + 安全承诺与限制的摘要（完整清单在仓库的 SECURITY.md）。
// 路径**只显示一处**（在「稿子放在哪」那一组）：两块各摊一遍绝对路径看着像出了错。
import { computed, onMounted, ref } from "vue";

import { t } from "../locales/index.ts";
import { openDataDir, readAppVersion, readDataHome, readEngineInfo } from "../api/core";
import type { EditorSession } from "../editor/session";
import { SETTINGS_SECTIONS, firstSection } from "../editor/settings-nav.ts";
import LocationDialog from "./LocationDialog.vue";
import SettingsResetConfirm from "./SettingsResetConfirm.vue";

const props = defineProps<{ session: EditorSession }>();
const {
  values,
  workValues,
  busy,
  close,
  setJumpToEnd,
  resetToDefault,
  setNaming,
  loadWork,
  namingPlan,
  previewNaming,
  applyNaming,
} = props.session.appearance;

// 左边一列分类，右边只显示当前这一类：设置项一多，摊成一长列就没法看了（分区表见 editor/settings-nav.ts）
const activeSection = ref(firstSection());

/** 「恢复默认」先问一句再动：它是不可逆的**全局**动作（偏好回到初始值，稿子一个字不动） */
const resetVisible = ref(false);
async function confirmReset(): Promise<void> {
  await resetToDefault();
  resetVisible.value = false;
}

/** 勾选框的当前值（读不出来就显示未勾选，并禁用） */
const jumpToEnd = computed(() => values.value?.jump_to_end_on_latest ?? false);
const unavailable = computed(() => values.value === null);

/** 稿子现在放在哪（壳报告的只读路径） */
const dataPath = computed(() => props.session.location.info.value?.path ?? "");

/** 「换位置」：先把现状读一遍再把这个面板请出来（它盖在设置上面那一层） */
const relocating = ref(false);
function startRelocate(): void {
  void props.session.location.load();
  relocating.value = true;
}

// ── 关于（只读报告；经 api 网关取数，组件不直接碰 Tauri）────────────
const appVersion = ref("");
const engineVersion = ref("");
const schemaVersion = ref<number | null>(null);
const aboutError = ref<string | null>(null);
/** 打开稿子文件夹失败的原因（点了没反应最难查，所以要把原因说出来） */
const openError = ref<string | null>(null);

/** 一行报全：安装包版本 / 引擎版本 / 库结构版本 */
const aboutVersion = computed(() =>
  t("settings.about_version_value", {
    version: appVersion.value || "—",
    engine: engineVersion.value || "—",
    schema: schemaVersion.value ?? "—",
  }),
);

async function openDataFolder(): Promise<void> {
  try {
    await openDataDir();
    openError.value = null;
  } catch (error) {
    openError.value = error instanceof Error ? error.message : String(error);
  }
}

onMounted(async () => {
  // 版本号单独兜底：读不到不该把整块"关于"打成错误
  appVersion.value = await readAppVersion().catch(() => "");
  try {
    const [engine, home] = await Promise.all([readEngineInfo(), readDataHome()]);
    engineVersion.value = engine.version;
    schemaVersion.value = home.schema_version;
  } catch (error) {
    aboutError.value = error instanceof Error ? error.message : String(error);
  }
});

/** 勾选框：改完写回核心，界面显示的永远是库里那份 */
function onToggle(event: Event) {
  const box = event.target as HTMLInputElement;
  void setJumpToEnd(box.checked);
}

// ── 新建条目的命名规则 ─────────────────────────────────────────
// 号是位置的函数（标题里写 `第{$N}章`，显示时按位置渲染）；这里选的是**新建时用哪套写法**。
// 写哪一层由"作用范围"决定：所有作品的默认 / 只设当前这本书（每书覆盖）。
const namingScope = ref<"default" | "work">("default");
const workTitle = computed(() => props.session.chapterTitle.value || t("common.untitled"));
const hasWork = computed(() => props.session.workId.value !== null);

/** 选择框显示的那一档：看当前作用范围，读不到就当"按作品类型" */
const namingValue = computed(() => {
  const source = namingScope.value === "work" ? workValues.value : values.value;
  return source?.naming ?? "auto";
});

const NAMING_OPTIONS = [
  { code: "auto", key: "settings.naming.auto" },
  { code: "arabic", key: "settings.naming.arabic" },
  { code: "chinese", key: "settings.naming.chinese" },
  { code: "padded", key: "settings.naming.padded" },
  { code: "none", key: "settings.naming.none" },
];

function onScope(event: Event) {
  const next = (event.target as HTMLSelectElement).value === "work" ? "work" : "default";
  namingScope.value = next;
  // 切到"只设这本书"时把它生效的那一份重读一下
  if (next === "work") void loadWork();
}

function onNaming(event: Event) {
  const code = (event.target as HTMLSelectElement).value;
  void setNaming(code, namingScope.value === "work" ? "work" : "default");
}

// ── 把已有章节一起换写法（显式动作：先预览再确认，绝不静默改稿）─────────────
const rewriteVisible = ref(false);
const rewriteBusy = ref(false);
const rewriteDone = ref<number | null>(null);

/** 预览里最多摆几条（多了就"另有 N 章"） */
const PREVIEW_LIMIT = 8;
const previewHead = computed(() => (namingPlan.value ?? []).slice(0, PREVIEW_LIMIT));
const previewRest = computed(() => Math.max(0, (namingPlan.value?.length ?? 0) - PREVIEW_LIMIT));

/** 命名规则刚改过、这本书又有要换的章，就把"一起换"这行提示摆出来 */
async function openRewrite() {
  await previewNaming();
  rewriteDone.value = null;
  rewriteVisible.value = true;
}

async function runRewrite() {
  rewriteBusy.value = true;
  try {
    const changed = await applyNaming();
    if (changed !== null) {
      rewriteDone.value = changed;
      rewriteVisible.value = false;
      // 目录树上的名字得跟着更新（会话那边的刷新入口）
      await props.session.directory.refresh();
    }
  } finally {
    rewriteBusy.value = false;
  }
}
</script>

<template>
  <div class="settings dialog" @click.self="close">
    <section class="settings__box dialog__box">
      <header class="settings__head dialog__head">
        <h2 class="settings__title dialog__title">{{ t("settings.title") }}</h2>
        <button type="button" class="settings__button dialog__button" @click="close">{{ t("common.close") }}</button>
      </header>

      <div class="settings__body">
        <nav class="settings__nav" :aria-label="t('settings.title')">
          <button
            v-for="section in SETTINGS_SECTIONS"
            :key="section.id"
            type="button"
            class="settings__nav-item"
            :class="{ 'settings__nav-item--on': activeSection === section.id }"
            :aria-current="activeSection === section.id ? 'true' : undefined"
            @click="activeSection = section.id"
          >
            {{ t(section.labelKey) }}
          </button>
        </nav>

        <div class="settings__right">
        <div class="settings__pane">
      <section v-if="activeSection === 'writing'" class="settings__section">
      <p class="settings__group">{{ t("settings.group_writing") }}</p>
      <label class="settings__row">
        <input
          type="checkbox"
          :checked="jumpToEnd"
          :disabled="busy || unavailable"
          @change="onToggle"
        />
        <span>{{ t("settings.jump_to_end") }}</span>
      </label>
      <p class="settings__hint">
        {{ t("settings.read_only_hint") }}
      </p>
      <p class="settings__hint">{{ t("settings.local_only_hint") }}</p>

      <!-- 新建条目：命名规则（写哪一层由"作用范围"决定） -->
      </section>
      <section v-if="activeSection === 'naming'" class="settings__section">
      <p class="settings__group settings__group--naming">{{ t("settings.group_naming") }}</p>
      <p class="settings__row">
        <span class="settings__label">{{ t("settings.naming_scope") }}</span>
        <select class="settings__select" :disabled="busy" @change="onScope">
          <option value="default" :selected="namingScope === 'default'">
            {{ t("settings.naming_scope_default") }}
          </option>
          <option value="work" :selected="namingScope === 'work'" :disabled="!hasWork">
            {{ hasWork ? t("settings.naming_scope_work", { title: workTitle }) : t("settings.naming_no_work") }}
          </option>
        </select>
      </p>
      <p class="settings__row">
        <span class="settings__label">{{ t("settings.naming") }}</span>
        <select class="settings__select" :disabled="busy" @change="onNaming">
          <option
            v-for="option in NAMING_OPTIONS"
            :key="option.code"
            :value="option.code"
            :selected="option.code === namingValue"
          >
            {{ t(option.key) }}
          </option>
        </select>
      </p>
      <p class="settings__hint">{{ t("settings.naming_hint") }}</p>
      <!-- 把已有章节一起换写法：显式动作 + 先预览（改的是作者的文字，绝不静默做） -->
      <button
        v-if="namingPlan !== null && namingPlan.length > 0"
        type="button"
        class="settings__button dialog__button"
        :disabled="busy"
        @click="void openRewrite()"
      >
        {{ t("settings.naming_rewrite", { title: workTitle, count: namingPlan.length }) }}
      </button>
      <p v-else-if="namingPlan !== null" class="settings__hint">
        {{ namingValue === "none" ? t("settings.naming_rewrite_plain") : t("settings.naming_rewrite_none") }}
      </p>
      <p v-if="rewriteDone !== null" class="settings__hint">
        {{ t("settings.naming_rewrite_done", { count: rewriteDone }) }}
      </p>
      <p class="settings__hint">{{ t("settings.naming_macro_hint") }}</p>
      <!-- i18n-allow-next-line: 自动编号宏的写法是**代码语法**（作者照抄用），不是可翻译的界面文案 -->
      <p class="settings__hint">
        <code>第{$N}章</code> · <code>第{$N_ZH}章</code> · <code>第{$N:3}章</code> ·
        <code>{$N_RESET:101}</code>
      </p>

      </section>
      <section v-if="activeSection === 'location'" class="settings__section">
      <p class="settings__group settings__group--data">{{ t("settings.group_data") }}</p>
      <p class="settings__row settings__row--path">
        <span class="settings__label">{{ t("location.current") }}</span>
        <code class="settings__path">{{ dataPath }}</code>
      </p>
      <p class="settings__hint">{{ t("location.change_later") }}</p>
      <button type="button" class="settings__button dialog__button" :disabled="!dataPath" @click="startRelocate">
        {{ t("location.change") }}
      </button>

      <p v-if="unavailable" class="settings__hint settings__hint--bad">
        {{ t("settings.unavailable") }}
      </p>

      </section>
      <!-- 关于：只读报告。承诺与限制都写在明面上——信任靠坦白边界建立 -->
      <section v-if="activeSection === 'about'" class="settings__section">
      <p class="settings__group">{{ t("settings.group_about") }}</p>
      <p class="settings__row">
        <span class="settings__label">{{ t("settings.about_version") }}</span>
        <code class="settings__path">{{ aboutVersion }}</code>
      </p>
      <!-- 路径**只在「稿子放在哪」那一组显示一次**：这里留着按钮就够了（它的标签已经说清是打开哪个文件夹）。
           两处都摊一遍绝对路径，看着像出了错，也让人不知道该信哪一处。 -->
      <button
        type="button"
        class="settings__button dialog__button"
        :disabled="!dataPath"
        @click="void openDataFolder()"
      >
        {{ t("settings.about_open_data") }}
      </button>
      <span v-if="openError" class="settings__hint settings__hint--bad">{{ openError }}</span>
      <p class="settings__hint">{{ t("settings.about_promise") }}</p>
      <p class="settings__hint">{{ t("settings.about_limits") }}</p>
      <p v-if="aboutError" class="settings__hint settings__hint--bad">{{ aboutError }}</p>
      </section>
        </div>

        <!-- 恢复默认：**只在「关于」这一栏**（用户 2026-09-14 真机反馈：每一栏都长一个太多）。
             位置放在右列底部、离「关闭」远一点：它是个不可逆的全局动作，挨着关闭按钮
             迟早有人手滑（真报过）。点它还会再问一次（见下面的确认层）。 -->
        <div v-if="activeSection === 'about'" class="settings__footer">
          <button
            type="button"
            class="settings__button dialog__button settings__button--danger"
            :disabled="busy"
            :title="t('settings.reset_title')"
            @click="resetVisible = true"
          >
            {{ t("settings.reset") }}…
          </button>
          <span class="settings__hint settings__hint--inline">{{ t("settings.reset_hint") }}</span>
        </div>
        </div>
      </div>

      <!-- 恢复默认的确认层（单独一个组件：面板别长胖，见那个文件头） -->
      <SettingsResetConfirm
        v-if="resetVisible"
        :busy="busy"
        @cancel="resetVisible = false"
        @confirm="confirmReset"
      />

      <!-- 预览：会改哪几章、改成什么（确认才动手） -->
      <div v-if="rewriteVisible" class="settings dialog dialog--above" @click.self="rewriteVisible = false">
        <section class="settings__box dialog__box">
          <header class="settings__head dialog__head">
            <h2 class="settings__title dialog__title">{{ t("settings.naming_rewrite_title") }}</h2>
            <button type="button" class="settings__button dialog__button" @click="rewriteVisible = false">
              {{ t("common.cancel") }}
            </button>
          </header>
          <p class="settings__hint">{{ t("settings.naming_rewrite_hint") }}</p>
          <ul class="settings__plan">
            <li v-for="item in previewHead" :key="item.node_id" class="settings__plan-row">
              <span class="settings__plan-before">{{ item.before }}</span>
              <span class="settings__plan-arrow">→</span>
              <span class="settings__plan-after">{{ item.after }}</span>
            </li>
          </ul>
          <p v-if="previewRest > 0" class="settings__hint">
            {{ t("settings.naming_rewrite_more", { count: previewRest }) }}
          </p>
          <button
            type="button"
            class="settings__button dialog__button"
            :disabled="rewriteBusy"
            @click="void runRewrite()"
          >
            {{ rewriteBusy ? t("settings.naming_rewrite_busy") : t("settings.naming_rewrite_confirm") }}
          </button>
        </section>
      </div>

      <LocationDialog v-if="relocating" :session="session" mode="settings" @close="relocating = false" />
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>
