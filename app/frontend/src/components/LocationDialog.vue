<script setup lang="ts">
// 「稿子放在哪」：首启引导（选位置 → 提醒备份）与设置里的"换位置"共用这一个面板。
//
// 三条分寸（与核心/壳那边的约定一一对应）：
// - **这一页只显示与转发**：复制、核对、写记录、拒绝危险目标全在壳与核心；
// - **路径只用来显示**：真正搬家时界面只说一句"搬过去"，不把路径递回壳；
// - **风险如实说**：同步盘/桌面/可移动盘都摆出来，但按不按得动是作者的事。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import { formatBytes } from "../editor/display.ts";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession; mode: "first-run" | "settings" }>();
const emit = defineEmits<{ (event: "close"): void }>();

const { info, picked, busy, error, moved, close, pickDir, useCurrent, confirmMove, forget } =
  props.session.location;

/** 首启是两步：先定位置，再提醒"别只放一处"；设置里进来只有位置那一步 */
const step = ref<"place" | "backup">("place");

const currentPath = computed(() => info.value?.path ?? "");
const suggestedReason = computed(() => {
  const reason = info.value?.suggested_reason;
  return reason ? t(`location.reason.${reason}`) : "";
});
/** 风险码 → 人话（认不出的码就不显示，别编一句糊上去） */
const riskKeys = computed(() =>
  (picked.value?.risks ?? []).map((risk) => `location.risk.${risk}`),
);
const canMove = computed(() => picked.value !== null && !busy.value && !moved.value);

function chosen(): void {
  void pickDir(t("location.pick_title"));
}

function startOver(): void {
  void forget();
  void pickDir(t("location.pick_title"));
}

/** 首启选「就用这里」：记下位置，接着走第二步（提醒别只放一处） */
async function accept(): Promise<void> {
  await useCurrent();
  if (error.value) return;
  step.value = "backup";
}

async function move(): Promise<void> {
  await confirmMove();
  // 搬成功：窗口马上重启，所以这里不再收起面板（保持忙态，不给再按一次的机会）
}

function dismiss(): void {
  close();
  emit("close");
}
</script>

<template>
  <div class="dialog" :class="{ 'dialog--above': mode === 'settings' }" @click.self="dismiss()">
    <section class="dialog__box location">
      <header class="dialog__head">
        <h2 class="dialog__title">
          {{ step === "place" ? t("location.title_place") : t("location.title_backup") }}
        </h2>
        <button type="button" class="dialog__button" @click="dismiss()">
          {{ t("common.close") }}
        </button>
      </header>

      <!-- 第一步：稿子放哪 -->
      <template v-if="step === 'place'">
        <p class="location__lead">{{ t("location.lead") }}</p>

        <p class="location__row">
          <span class="location__label">{{ t("location.current") }}</span>
          <code class="location__path">{{ currentPath }}</code>
        </p>
        <p v-if="suggestedReason && mode === 'first-run'" class="location__hint">
          {{ suggestedReason }}
        </p>
        <p class="location__hint">{{ t("location.change_later") }}</p>

        <template v-if="picked">
          <p class="location__row">
            <span class="location__label">{{ t("location.picked") }}</span>
            <code class="location__path">{{ picked.path }}</code>
          </p>
          <ul v-if="riskKeys.length" class="location__risks">
            <li v-for="key in riskKeys" :key="key" class="location__risk">{{ t(key) }}</li>
          </ul>
          <p v-if="picked.entries > 0" class="location__hint">
            {{ t("location.not_empty", { count: String(picked.entries) }) }}
          </p>
          <p class="location__hint">{{ t("location.keeps_old") }}</p>
        </template>

        <p v-if="moved" class="location__hint location__ok">
          {{
            t("location.moved", {
              path: moved.path,
              files: String(moved.files),
              size: formatBytes(moved.bytes),
            })
          }}
        </p>

        <p v-if="error" class="location__bad">{{ error }}</p>

        <footer class="location__actions">
          <template v-if="picked">
            <button type="button" class="dialog__button" :disabled="busy" @click="startOver">
              {{ t("location.choose_again") }}
            </button>
            <button
              type="button"
              class="dialog__button location__primary"
              :disabled="!canMove"
              @click="void move()"
            >
              {{ busy ? t("location.moving") : t("location.move_here") }}
            </button>
          </template>
          <template v-else>
            <button type="button" class="dialog__button" :disabled="busy" @click="chosen">
              {{ t("location.choose_other") }}
            </button>
            <button
              v-if="mode === 'first-run'"
              type="button"
              class="dialog__button location__primary"
              :disabled="busy"
              @click="void accept()"
            >
              {{ t("location.use_this") }}
            </button>
          </template>
        </footer>
      </template>

      <!-- 第二步（只有首启）：别只放一处 -->
      <template v-else>
        <p class="location__lead">{{ t("location.backup_lead") }}</p>
        <p class="location__hint">{{ t("location.backup_hint") }}</p>
        <footer class="location__actions">
          <button type="button" class="dialog__button" @click="dismiss()">
            {{ t("location.backup_later") }}
          </button>
          <button
            type="button"
            class="dialog__button location__primary"
            @click="
              () => {
                session.backup.open();
                dismiss();
              }
            "
          >
            {{ t("location.backup_open") }}
          </button>
        </footer>
      </template>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped>
.location__lead {
  margin: 0 0 8px;
  font-size: 13px;
}

.location__row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin: 6px 0;
  font-size: 12px;
}

.location__label {
  flex: 0 0 auto;
  color: var(--ym-ink-soft);
}

.location__path {
  word-break: break-all;
}

.location__hint {
  margin: 4px 0;
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.location__risks {
  margin: 6px 0;
  padding-left: 18px;
  font-size: 12px;
}

.location__risk {
  color: var(--ym-accent);
}

.location__ok {
  color: var(--ym-accent);
}

.location__bad {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ym-accent);
}

.location__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}

.location__primary {
  border-color: var(--ym-accent);
  color: var(--ym-accent);
}
</style>
