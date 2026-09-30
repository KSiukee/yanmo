<script setup lang="ts">
// 顶栏：**一个品牌名 + 几个入口 + 一行小字**。
//
// 单独成件是因为它跟"三栏怎么摆"是两个变化理由：顶栏会长按钮（每加一个入口就多一个），
// 而布局的骨架基本不动。抽出来之后布局那一份只说"露不露"，不必跟着按钮一起长。
//
// 按钮**只有这一排**（顺序就是作者找它的顺序）：书架 → 检索 → 稿件台 → 专注 → 设定 → 备份 → 设置。
// 它不做任何业务：点哪个都只是把意图报给布局层（那边才知道这些弹窗归谁管）。
import { t } from "../locales/index.ts";
import { shortcutKeys } from "../editor/shortcuts.ts";
import EngineBadge from "./EngineBadge.vue";

defineProps<{
  /** 开着书没有（书架按钮说什么、小字显示什么，都看它） */
  workId: number | null;
  /** 顶栏那行小字：有章名就章名；开着书没起名就占位；没书才说立场那句话 */
  hint: string;
  /** 投稿已超期、可以改投的篇数（0 = 不显示徽标） */
  manuscriptOverdue: number;
}>();

const emit = defineEmits<{
  shelf: [];
  search: [];
  manuscript: [];
  zen: [];
  grid: [];
  entities: [];
  backup: [];
  settings: [];
}>();
</script>

<template>
  <header class="bar">
    <!-- i18n-allow-next-line: 产品名（品牌），不是界面文案 -->
    <span class="bar__brand">研墨</span>
    <button
      type="button"
      class="bar__button"
      :title="workId ? t('app.shelf_title_switch') : t('app.shelf_title_open')"
      @click="emit('shelf')"
    >
      {{ t("app.shelf") }}
    </button>
    <button
      type="button"
      class="bar__button"
      :title="t('app.search_title')"
      @click="emit('search')"
    >
      {{ t("app.search") }}
    </button>
    <button
      type="button"
      class="bar__button"
      :title="
        manuscriptOverdue > 0
          ? t('app.manuscript_overdue_title', { count: manuscriptOverdue })
          : t('app.manuscript_title')
      "
      @click="emit('manuscript')"
    >
      {{ t("app.manuscript") }}
      <!-- 超期未回的投稿：**安安静静一个数字**，不弹窗、不打断（点开才说详情） -->
      <span v-if="manuscriptOverdue > 0" class="bar__badge">{{ manuscriptOverdue }}</span>
    </button>
    <button
      type="button"
      class="bar__button"
      :title="t('app.zen_title', { key: shortcutKeys('zen') })"
      @click="emit('zen')"
    >
      {{ t("app.zen") }}
    </button>
    <button
      type="button"
      class="bar__button"
      :title="t('app.grid_title')"
      @click="emit('grid')"
    >
      {{ t("app.grid") }}
    </button>
    <button
      type="button"
      class="bar__button"
      :title="t('app.entities_title')"
      @click="emit('entities')"
    >
      {{ t("app.entities") }}
    </button>
    <button
      type="button"
      class="bar__button"
      :title="t('app.backup_title')"
      @click="emit('backup')"
    >
      {{ t("app.backup") }}
    </button>
    <button
      type="button"
      class="bar__button"
      :title="t('app.settings_title')"
      @click="emit('settings')"
    >
      {{ t("app.settings") }}
    </button>
    <span class="bar__hint">{{ hint }}</span>
    <EngineBadge />
  </header>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: baseline;
  gap: 12px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--ym-line);
  background: var(--ym-paper-dim);
}

.bar__brand {
  font-weight: 600;
  letter-spacing: 0.08em;
}

.bar__button {
  padding: 1px 10px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper);
  color: inherit;
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.bar__button:hover {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}

.bar__hint {
  font-size: 12px;
  color: var(--ym-ink-soft);
}

/* 徽标：数字贴着按钮右边，不占布局（按钮自己开着 gap） */
.bar__badge {
  margin-left: 4px;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--ym-accent);
  color: var(--ym-paper);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
</style>
