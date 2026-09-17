<script setup lang="ts">
// 磁盘 .md 镜像的**待定夺清单**：启动时（或作者从设置里点进来时）发现"磁盘上那一份
// 与研墨这一边不一样了"，就把作者请过来，逐条给两条路：
//
//   · 收进研墨：磁盘上那份的字进库；**库里原来那一版会先留成快照**（版本历史里看得到），
//     谁都不丢；
//   · 用研墨的版本盖回去：磁盘上那份被删掉，镜像随后按库里的字重写。
//
// 没账的文件（多半是作者自己挪过名字）**只列不动**——研墨不碰自己没有账的东西，
// 只把路径摆出来让他自己看。
//
// 纪律：界面从头到尾**不碰文件系统**，也**不传路径**给壳——只送"第几条"与那个节点 id。
import { ref } from "vue";

import { t } from "../locales/index.ts";
import { issueActions, issueKey } from "../editor/mirror.ts";
import { closeMirrorReview, refreshMirror, useMirrorReview } from "../editor/mirror-review.ts";
import type { EditorSession } from "../editor/session";
import type { MirrorIssue } from "../api/mirror";

const props = defineProps<{ session: EditorSession }>();
const { status, error } = useMirrorReview();
const issues = () => status.value?.issues ?? [];

const busy = ref(false);
const actionError = ref<string | null>(null);

/** 处置一条：会话那边定死顺序（先落盘 → 处置 → 采纳后重读当前章）。 */
async function act(index: number, issue: MirrorIssue, action: "adopt" | "overwrite"): Promise<void> {
  busy.value = true;
  actionError.value = null;
  try {
    await props.session.resolveMirror(index, issue.node_id, action);
    await refreshMirror();
    if ((status.value?.issues.length ?? 0) === 0) closeMirrorReview();
  } catch (failure) {
    actionError.value = failure instanceof Error ? failure.message : String(failure);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="mirror dialog" @click.self="closeMirrorReview()">
    <section class="settings__box dialog__box">
      <header class="settings__head dialog__head">
        <h2 class="settings__title dialog__title">{{ t("mirror.title") }}</h2>
        <button type="button" class="settings__button dialog__button" @click="closeMirrorReview()">
          {{ t("common.close") }}
        </button>
      </header>

      <p class="settings__hint">{{ t("mirror.intro") }}</p>

      <ul v-if="issues().length > 0" class="mirror__list">
        <li v-for="(issue, index) in issues()" :key="issueKey(issue)" class="mirror__row">
          <div class="mirror__what">
            <span class="mirror__name">{{ issue.title || t("mirror.untracked_name") }}</span>
            <code class="mirror__path">{{ issue.relative_path }}</code>
          </div>
          <div class="mirror__actions">
            <template v-if="issueActions(issue).adopt">
              <button
                type="button"
                class="settings__button dialog__button"
                :disabled="busy"
                @click="void act(index, issue, 'adopt')"
              >
                {{ t("mirror.adopt") }}
              </button>
              <button
                type="button"
                class="settings__button dialog__button"
                :disabled="busy"
                @click="void act(index, issue, 'overwrite')"
              >
                {{ t("mirror.overwrite") }}
              </button>
            </template>
            <span v-else class="mirror__note">{{ t("mirror.untracked_note") }}</span>
          </div>
        </li>
      </ul>
      <p v-else class="settings__hint">{{ t("mirror.all_clear") }}</p>

      <p class="settings__hint">{{ t("mirror.hint_adopt") }}</p>
      <p class="settings__hint">{{ t("mirror.hint_untracked") }}</p>
      <p v-if="actionError" class="settings__hint settings__hint--bad">{{ actionError }}</p>
      <p v-else-if="error" class="settings__hint settings__hint--bad">{{ error }}</p>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./settings-dialog.css"></style>
<style scoped src="./mirror-dialog.css"></style>
