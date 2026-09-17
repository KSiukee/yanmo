<script setup lang="ts">
// 书架：**多作品是默认形态**——这一屏就是"我手上有哪几本书、各写到什么程度"。
//
// 只在打开时拉一次列表（书架不是常驻画面），切书交给会话层（先落盘再切）。
import { ref } from "vue";

import type { EditorSession } from "../editor/session";
import { shelfKindLabel, shelfLabel } from "../editor/shelf";
import type { ShelfEntry } from "../api/core";
import { t } from "../locales/index.ts";
import { formatWhen } from "../editor/display";

const props = defineProps<{ session: EditorSession }>();
const { entries, busy, close, open, remove, export: exportWork, note, openCreate, openEdit } =
  props.session.shelf;
const { toggle: toggleTrash } = props.session.trash;
const { workId, caliber } = props.session;
const { open: openCompile } = props.session.compile;

/** 去回收站：先把书架收起来，免得两层弹层叠在一起 */
function openTrash() {
  close();
  toggleTrash();
}

/** 展开了"更多"动作的那一本（同时只可能有一本）：日常只用「打开 / 编译」，
 *  导出、编辑、删除这些低频动作收进去，免得一行挤六个按钮。
 *
 *  按钮次序（用户 2026-09-14 真机定的）：`编译 | 编辑 导出 删除 | 收起`——
 *  「收起」这个开合开关挪到最右边，三种低频动作紧跟在「编译」后面。
 *  注意它是**同一个元素**：收起时它排在第 2 位（紧跟「编译」），展开后跑到第 5 位。 */
const expanded = ref<number | null>(null);

function toggleMore(work_id: number) {
  expanded.value = expanded.value === work_id ? null : work_id;
}

/** 书架上的书名：没起名的显示占位（默认名不落库，名字由作者起） */
function workLabel(title: string): string {
  return title || t("shelf.untitled_work");
}

/** 打开作品表单：新建 / 编辑都由**同一张表单**问（会话层持有开合状态，
 *  因为首启那条提示上的「建一本书」也要能打开它）。 */
function startCreate() {
  openCreate();
}

function startEdit(entry: ShelfEntry) {
  openEdit(entry);
}

/** 删书是不可逆的入口（虽然库里是软删），问一句再动手 */
function confirmRemove(work_id: number, title: string) {
  if (window.confirm(t("shelf.delete_confirm", { title: workLabel(title) }))) {
    void remove(work_id);
  }
}
</script>

<template>
  <div class="shelf dialog" @click.self="close">
    <section class="shelf__box dialog__box">
      <header class="shelf__head dialog__head">
        <h2 class="shelf__title dialog__title">{{ t("shelf.title") }}</h2>
        <button type="button" class="shelf__button dialog__button" :disabled="busy" @click="startCreate">
          {{ t("shelf.new_work") }}
        </button>
        <button type="button" class="shelf__button dialog__button" :title="t('shelf.trash_title')" @click="openTrash">
          {{ t("shelf.trash") }}
        </button>
        <button type="button" class="shelf__button dialog__button" :title="t('shelf.close_title')" @click="close">{{ t("common.close") }}</button>
      </header>

      <!-- 上一次动作的交代（"导出到哪儿了"之类）：与回收站 / 版本那两个弹窗同一条样式。
           这一行在 v0.39.0 的建书页重构里被误删过——导出照样写文件，只是屏幕上
           再没人告诉作者"导出到哪儿了"，看着就像按钮失效（用户报上来的就是这个）。 -->
      <p v-if="note" class="shelf__note dialog__note">{{ note }}</p>

      <ul class="shelf__list">
        <li
          v-for="entry in entries"
          :key="entry.id"
          class="shelf__card"
          :class="{ 'shelf__card--current': entry.id === workId }"
        >
          <div class="shelf__main">
            <span class="shelf__name" :title="workLabel(entry.title)">{{ workLabel(entry.title) }}</span>
            <span class="shelf__meta">
              {{ shelfKindLabel(entry.kind) }} · {{ shelfLabel(entry, caliber) }} ·
              {{ formatWhen(entry.opened_at) }}
            </span>
            <!-- 简介：写了就显示一行（改它去「编辑」那张表单），没写就不占地方 -->
            <span v-if="entry.summary" class="shelf__summary-line" :title="entry.summary">
              {{ entry.summary }}
            </span>
          </div>

          <div class="shelf__actions">
            <span v-if="entry.id === workId" class="shelf__here">{{ t("shelf.writing_now") }}</span>
            <button
              v-else
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              @click="open(entry.id)"
            >
              {{ t("shelf.open") }}
            </button>
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              :title="t('compile.button_title')"
              @click="void openCompile(entry.id)"
            >
              {{ t("compile.button") }}
            </button>
            <!-- 低频动作：收在「更多」里（编辑 → 导出 → 删除） -->
            <template v-if="expanded === entry.id">
              <button
                type="button"
                class="shelf__button dialog__button"
                :disabled="busy"
                :title="t('shelf.edit_title')"
                @click="startEdit(entry)"
              >
                {{ t("shelf.edit") }}
              </button>
              <button
                type="button"
                class="shelf__button dialog__button"
                :disabled="busy"
                :title="t('shelf.export_title')"
                @click="exportWork(entry.id, 'both')"
              >
                {{ t("shelf.export") }}
              </button>
              <button
                type="button"
                class="shelf__button shelf__button--danger dialog__button"
                :disabled="busy"
                @click="confirmRemove(entry.id, entry.title)"
              >
                {{ t("shelf.delete") }}
              </button>
            </template>

            <!-- 开合开关：**排在最后**（用户真机定的次序） -->
            <button
              type="button"
              class="shelf__button dialog__button"
              :disabled="busy"
              :title="t('shelf.more_title')"
              @click="toggleMore(entry.id)"
            >
              {{ expanded === entry.id ? t("shelf.more_close") : t("shelf.more") }}
            </button>
          </div>
        </li>
      </ul>
    </section>

  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./shelf-dialog.css"></style>
