<script setup lang="ts">
// 从备份恢复：挑一份备份 → 看清会丢什么 → 确认换库。
//
// 与别的弹窗同一条纪律：**这一页只显示与转发**——体检、留底、回滚、换库都在核心。
// 界面不自己判断"这份看着还行"：能不能按，只看核心给的 `can_restore`。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import { formatBytes, formatWords } from "../editor/display.ts";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
const { sources, picked, preview, busy, error, done, close, select, pickDatabase, apply } =
  props.session.restore;

/** 作者勾了"我知道"才让按——换库是这份软件里最重的一个动作 */
const confirmed = ref(false);

const sizeText = (bytes: number): string => formatBytes(bytes);

/** 时间给人看：要精确到分（"哪一份更新"就靠它分辨） */
function when(ms: number): string {
  const date = new Date(ms);
  return Number.isFinite(date.getTime()) ? date.toLocaleString() : t("restore.time_unknown");
}

const canApply = computed(
  () => preview.value?.can_restore === true && confirmed.value && !busy.value && !done.value,
);

/** 换一份来源就要重新确认：上一个"我知道"不该替这一份背书 */
async function choose(source: string): Promise<void> {
  confirmed.value = false;
  await select(source);
}

async function chooseFromDisk(): Promise<void> {
  confirmed.value = false;
  await pickDatabase(t("restore.pick_title"), t("restore.pick_filter"));
}
</script>

<template>
  <div class="dialog" @click.self="close()">
    <section class="dialog__box restore">
      <header class="dialog__head">
        <h2 class="dialog__title">{{ t("restore.title") }}</h2>
        <button type="button" class="dialog__button" :title="t('restore.close_title')" @click="close()">
          {{ t("common.close") }}
        </button>
      </header>

      <!-- 换完库的交代：窗口马上会重启，这句话要说清"接下来会发生什么" -->
      <template v-if="done">
        <p class="restore__ok">{{ t("restore.done") }}</p>
        <p v-if="done.quarantine" class="dialog__note">
          {{ t("restore.done_keep", { dir: done.quarantine }) }}
        </p>
        <p class="dialog__note">{{ t("restore.restarting") }}</p>
      </template>

      <template v-else>
        <p class="dialog__note">{{ t("restore.intro") }}</p>

        <h3 class="restore__section">{{ t("restore.packages") }}</h3>
        <ul v-if="sources?.packages.length" class="restore__list">
          <li
            v-for="item in sources.packages"
            :key="item.path"
            class="restore__row"
            :class="{ 'restore__row--picked': picked === item.path }"
          >
            <div class="restore__main">
              <span class="restore__label">{{ when(item.created_at) }}</span>
              <span class="restore__meta">
                {{
                  t("restore.package_meta", {
                    works: item.works,
                    chapters: item.chapters,
                    words: formatWords(item.words),
                  })
                }}
                <template v-if="item.device">
                  · {{ t("restore.package_device", { device: item.device }) }}
                </template>
                <template v-if="sizeText(item.bytes)"> · {{ sizeText(item.bytes) }}</template>
              </span>
            </div>
            <button type="button" class="dialog__button" :disabled="busy" @click="choose(item.path)">
              {{ picked === item.path ? t("restore.choose_again") : t("restore.choose") }}
            </button>
          </li>
        </ul>
        <p v-else class="dialog__empty">{{ t("restore.no_packages") }}</p>

        <p class="restore__pick">
          <button type="button" class="dialog__button" :disabled="busy" @click="chooseFromDisk()">
            {{ t("restore.pick") }}
          </button>
          <span class="restore__hint">{{ t("restore.pick_hint") }}</span>
        </p>

        <!-- 选中之后的体检结论：先摆事实，再让作者确认 -->
        <template v-if="preview">
          <h3 class="restore__section">{{ t("restore.health") }}</h3>
          <p v-if="preview.verify.ok" class="restore__ok">{{ t("restore.verify_ok") }}</p>
          <template v-else>
            <p class="restore__bad">{{ t("restore.verify_failed") }}</p>
            <ul class="restore__problems">
              <li v-for="(problem, index) in preview.verify.problems" :key="index">{{ problem }}</li>
            </ul>
          </template>
          <p v-if="preview.is_live_database" class="restore__bad">{{ t("restore.is_live") }}</p>

          <ul class="restore__facts">
            <li>
              {{
                t("restore.fact_from", {
                  when: when(preview.created_at),
                  device: preview.device || t("restore.device_unknown"),
                })
              }}
            </li>
            <li>
              {{
                t("restore.fact_has", {
                  works: preview.source_works,
                  chapters: preview.source_chapters,
                  words: formatWords(preview.source_words),
                })
              }}
            </li>
            <li>{{ t("restore.fact_last_write", { when: when(preview.source_last_write_at) }) }}</li>
            <li v-if="preview.live_readable">
              {{
                t("restore.fact_now", {
                  works: preview.live_works,
                  words: formatWords(preview.live_words),
                  when: when(preview.live_last_write_at),
                })
              }}
            </li>
          </ul>

          <template v-if="preview.can_restore">
            <p v-if="!preview.live_readable" class="restore__warn">{{ t("restore.live_unreadable") }}</p>
            <p v-else-if="preview.lost_days || preview.lost_words" class="restore__warn">
              {{
                t("restore.will_lose", {
                  days: preview.lost_days,
                  words: formatWords(preview.lost_words),
                })
              }}
            </p>
            <p v-else class="dialog__note">{{ t("restore.will_lose_none") }}</p>
            <p class="dialog__note">{{ t("restore.keep", { dir: preview.keep_dir }) }}</p>
            <label class="restore__confirm">
              <input v-model="confirmed" type="checkbox" />
              {{ t("restore.confirm") }}
            </label>
            <div class="restore__actions">
              <button
                type="button"
                class="dialog__button dialog__button--danger"
                :disabled="!canApply"
                @click="apply()"
              >
                {{ busy ? t("restore.applying") : t("restore.apply") }}
              </button>
            </div>
          </template>
          <p v-else class="restore__bad">{{ t("restore.cannot_restore") }}</p>
        </template>

        <p v-else-if="busy" class="dialog__note">{{ t("restore.checking") }}</p>
      </template>

      <p v-if="error" class="restore__bad">{{ error }}</p>
    </section>
  </div>
</template>

<!-- 公共壳（遮罩 / 盒子 / 标题 / 按钮）：**必须引**，否则弹窗就是一堆裸内容叠在正文上 -->
<style scoped src="./dialog.css"></style>
<style scoped>
.restore {
  width: min(720px, 100%);
  overflow: auto;
}

.restore__section {
  margin: 14px 0 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--ym-ink-soft);
}

.restore__list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.restore__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
}

.restore__row--picked {
  border-color: var(--ym-accent);
}

.restore__main {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.restore__label {
  font-size: 12px;
}

.restore__meta,
.restore__hint {
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.restore__pick {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
}

.restore__facts,
.restore__problems {
  margin: 0 0 8px;
  padding-left: 18px;
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.restore__problems {
  color: #b3261e;
}

.restore__ok {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--ym-accent);
}

.restore__bad {
  margin: 0 0 8px;
  overflow-wrap: anywhere;
  font-size: 12px;
  color: #b3261e;
}

.restore__warn {
  margin: 0 0 8px;
  padding: 4px 8px;
  border-radius: 4px;
  background: var(--ym-paper-dim);
  font-size: 12px;
  font-weight: 600;
  color: #b3261e;
}

.restore__confirm {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px 0;
  font-size: 12px;
}

.restore__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
