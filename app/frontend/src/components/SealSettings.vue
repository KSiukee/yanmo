<script setup lang="ts">
// 备份设置页里的**加密备份包**那一组：开关 / 口令 / 恢复码。
//
// 为什么单独成组件：备份设置页本身贴着 400 行那条上帝棘轮，而这一块自成一体
//（自有状态机 `editor/seal.ts`，与别的设置项没有耦合）。
//
// 三条分寸写在最前面：
// - **界面只显示壳报的事实**：每个动作之后都把现状**读回来**（状态机里做），不自己推断
//   "应该已经开了"；
// - **恢复码那张卡必须作者自己确认**：那是唯一一次能抄下来的机会（包里取不回来），
//   界面不许替他点掉；
// - **口令不校验**：`给口令` 只是把它交上去记住——对不对要等真去开一个包才知道，
//   这句话必须在界面上说出来（见 backup.seal_unlocked_hint）。
import { computed, ref } from "vue";

import { t } from "../locales/index.ts";
import type { SealState } from "../editor/seal";

const props = defineProps<{ seal: SealState }>();
const { status, busy, error, showRecovery, recoveryCode, recoveryExists, enable, disable, unlock, lock, confirmRecovery } =
  props.seal;

/** 口令草稿：只在点按钮的那一瞬间交上去，**不留任何副本** */
const passphrase = ref("");
/** 第二遍（只在"开启"时要：打错一次就整包打不开了） */
const again = ref("");
/** 本地拦下的问题（两次不一致 / 空口令）——与壳报回来的错分开摆 */
const problem = ref<string | null>(null);
const copied = ref(false);
/** 恢复码那串字（复制不了就把它整段选中，让作者自己按 Ctrl+C） */
const codeEl = ref<HTMLElement | null>(null);

const enabled = computed(() => status.value?.enabled ?? false);
const unlocked = computed(() => status.value?.unlocked ?? false);

/** 开启：两次一致 + 非空，**先在这里拦**（犯不着跑一趟 IPC 再被壳拒收）。 */
async function startEnable(): Promise<void> {
  if (passphrase.value.trim() === "") {
    problem.value = t("backup.seal_empty");
    return;
  }
  if (passphrase.value !== again.value) {
    problem.value = t("backup.seal_mismatch");
    return;
  }
  problem.value = null;
  await enable(passphrase.value);
  passphrase.value = "";
  again.value = "";
}

/** 给口令（重启之后就是这条路）：**只是记住它**，对不对要等真去开一个包才知道。 */
async function givePassphrase(): Promise<void> {
  if (passphrase.value.trim() === "") {
    problem.value = t("backup.seal_empty");
    return;
  }
  problem.value = null;
  await unlock(passphrase.value);
  passphrase.value = "";
}

/** 没有剪贴板（或复制失败）：把那串码**整段选中**，作者自己按 Ctrl+C。 */
function selectCode(): void {
  const element = codeEl.value;
  if (!element) return;
  const range = document.createRange();
  range.selectNodeContents(element);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

/** 复制恢复码：有剪贴板就用它，没有就让文字保持选中（手抄也不算费事）。 */
async function copyCode(): Promise<void> {
  copied.value = false;
  const clipboard = navigator.clipboard;
  if (clipboard?.writeText) {
    try {
      await clipboard.writeText(recoveryCode.value);
      copied.value = true;
      return;
    } catch {
      // 复制不成不算错：走下面的"选中整串"兜底
    }
  }
  selectCode();
}

void props.seal.load();
</script>

<template>
  <h3 class="seal__section">{{ t("backup.seal_title") }}</h3>
  <p class="dialog__note">{{ t("backup.seal_intro") }}</p>

  <!-- 没开：一句话说清"现在写到盘上的包是明文"；读不到现状时这几档都不显示（见下） -->
  <template v-if="status && !enabled">
    <p class="dialog__note dialog__note--warn">{{ t("backup.seal_off") }}</p>
    <div class="seal__form">
      <label class="seal__row">
        {{ t("backup.seal_passphrase") }}
        <input v-model="passphrase" class="seal__text dialog__input" type="password" />
      </label>
      <label class="seal__row">
        {{ t("backup.seal_passphrase_again") }}
        <input v-model="again" class="seal__text dialog__input" type="password" />
      </label>
      <button
        type="button"
        class="dialog__button dialog__button--danger"
        :disabled="busy"
        @click="startEnable()"
      >
        {{ t("backup.seal_enable") }}
      </button>
    </div>
    <p class="seal__hint">{{ t("backup.seal_enable_hint") }}</p>
  </template>

  <!-- 开了 + 口令在这一次运行里 -->
  <template v-else-if="status && unlocked">
    <p class="dialog__note">{{ t("backup.seal_on") }}</p>
    <div class="seal__form">
      <button type="button" class="dialog__button" :disabled="busy" @click="lock()">
        {{ t("backup.seal_lock") }}
      </button>
      <button type="button" class="dialog__button" :disabled="busy" @click="disable()">
        {{ t("backup.seal_disable") }}
      </button>
    </div>
    <p class="seal__hint">{{ t("backup.seal_unlocked_hint") }}</p>
  </template>

  <!-- 开了 **但口令不在**（重启之后就是这种状态）：这一条必须醒目 -->
  <!-- 读不到现状（status 为 null）时这几档都不显示：宁可少显示，也别拿猜测当真 -->
  <template v-else-if="status">
    <p class="seal__warn">{{ t("backup.seal_locked_warning") }}</p>
    <div class="seal__form">
      <label class="seal__row">
        {{ t("backup.seal_passphrase") }}
        <input v-model="passphrase" class="seal__text dialog__input" type="password" />
      </label>
      <button type="button" class="dialog__button" :disabled="busy" @click="givePassphrase()">
        {{ t("backup.seal_unlock") }}
      </button>
      <button type="button" class="dialog__button" :disabled="busy" @click="disable()">
        {{ t("backup.seal_disable") }}
      </button>
    </div>
  </template>

  <p class="seal__hint">{{ t("backup.seal_disable_hint") }}</p>
  <p v-if="recoveryExists" class="seal__hint">{{ t("backup.seal_recovery_exists") }}</p>
  <p v-if="problem" class="seal__bad">{{ problem }}</p>
  <p v-if="error" class="seal__bad">{{ error }}</p>

  <!-- 恢复码：**必须作者自己按「我已抄到别处」才收起**（包里取不回来） -->
  <div v-if="showRecovery" class="seal__recovery">
    <p class="seal__recovery-title">{{ t("backup.seal_recovery_title") }}</p>
    <code ref="codeEl" class="seal__code">{{ recoveryCode }}</code>
    <p class="seal__warn">{{ t("backup.seal_recovery_warning") }}</p>
    <div class="seal__form">
      <button type="button" class="dialog__button" @click="copyCode()">
        {{ t("backup.seal_recovery_copy") }}
      </button>
      <span v-if="copied" class="seal__hint">{{ t("backup.seal_recovery_copied") }}</span>
      <button type="button" class="dialog__button dialog__button--danger" @click="confirmRecovery()">
        {{ t("backup.seal_recovery_confirm") }}
      </button>
    </div>
  </div>
</template>

<!-- 公共壳里的按钮 / 交代条 / 输入框：**必须引自己的那一份**（scoped 不会从父组件漏下来） -->
<style scoped src="./dialog.css"></style>
<style scoped>
.seal__section {
  margin: 14px 0 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--ym-ink-soft);
}

.seal__form {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
}

.seal__row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}

.seal__text {
  min-width: 160px;
}

.seal__hint {
  margin: 0 0 8px;
  overflow-wrap: anywhere;
  font-size: 12px;
  color: var(--ym-ink-soft);
}

.seal__bad {
  margin: 0 0 8px;
  overflow-wrap: anywhere;
  font-size: 12px;
  color: var(--ym-danger);
}

.seal__warn {
  margin: 0 0 8px;
  padding: 4px 8px;
  border-radius: 4px;
  background: var(--ym-paper-dim);
  font-size: 12px;
  font-weight: 600;
  color: var(--ym-warn);
}

.seal__recovery {
  margin: 10px 0 8px;
  padding: 10px;
  border: 1px solid var(--ym-accent);
  border-radius: 6px;
}

.seal__recovery-title {
  margin: 0 0 6px;
  font-size: 13px;
  font-weight: 600;
}

.seal__code {
  display: block;
  margin: 0 0 8px;
  padding: 6px 8px;
  border: 1px solid var(--ym-line);
  border-radius: 4px;
  background: var(--ym-paper-dim);
  font-family: ui-monospace, Consolas, monospace;
  font-size: 14px;
  letter-spacing: 0.08em;
  /* 复制不了时靠"整段选中"兜底：点一下就能全选，作者自己按 Ctrl+C */
  user-select: all;
  overflow-wrap: anywhere;
}
</style>
