<script setup lang="ts">
// 编译面板：选一种成品 → 看清会生成哪些文件 → 编译。
//
// 分工：预设清单、默认参数、预览与落盘全在 editor/compile.ts 与核心；
// 这个文件只管"长什么样"（与排版清理面板同一套骨架：先看后动）。
import { computed } from "vue";

import { has, t } from "../locales/index.ts";
import { fileSize, presetOf } from "../editor/compile";
import type { EditorSession } from "../editor/session";

const props = defineProps<{ session: EditorSession }>();
const {
  presets,
  preset,
  bodyLimit,
  withOutline,
  files,
  note,
  busy,
  close,
  selectPreset,
  setBodyLimit,
  setOutline,
  preview,
  run,
  openFolder,
} = props.session.compile;

/** 预设名在字典里（核心只给码）。 */
function presetName(code: string): string {
  const key = `compile.preset.${code}`;
  return t(has(key) ? key : code);
}

const current = computed(() => presetOf(presets.value, preset.value));

/** 正文上限：空着就是"全书都带"，只收数字。 */
const limitText = computed({
  get: () => (bodyLimit.value === null ? "" : String(bodyLimit.value)),
  set: (value: string) => {
    const digits = value.replace(/[^0-9]/g, "");
    setBodyLimit(digits === "" ? null : Number(digits));
  },
});

function onOutline(event: Event) {
  setOutline((event.target as HTMLInputElement).checked);
}
</script>

<template>
  <div class="compile dialog" @click.self="close()">
    <section class="compile__box dialog__box">
      <header class="dialog__head">
        <h2 class="dialog__title">{{ t("compile.title") }}</h2>
        <button type="button" class="dialog__button" @click="close()">
          {{ t("common.close") }}
        </button>
      </header>

      <p class="dialog__note">{{ t("compile.lead") }}</p>

      <h3 class="compile__section">{{ t("compile.preset") }}</h3>
      <ul class="compile__presets">
        <li v-for="item in presets" :key="item.code">
          <label class="compile__preset" :class="{ 'compile__preset--on': item.code === preset }">
            <input
              type="radio"
              name="compile-preset"
              :checked="item.code === preset"
              @change="selectPreset(item.code)"
            />
            <span>{{ presetName(item.code) }}</span>
          </label>
        </li>
      </ul>

      <div v-if="current?.caps_body" class="compile__params">
        <label class="compile__field">
          {{ t("compile.body_limit") }}
          <input
            v-model="limitText"
            class="compile__number"
            type="text"
            inputmode="numeric"
            :placeholder="t('compile.body_limit_whole')"
          />
          {{ t("compile.body_limit_unit") }}
        </label>
        <label class="compile__check">
          <input type="checkbox" :checked="withOutline" @change="onOutline" />
          {{ t("compile.outline") }}
        </label>
        <p class="compile__hint">{{ t("compile.body_limit_hint") }}</p>
      </div>

      <div class="compile__preview">
        <button type="button" class="dialog__button" :disabled="busy" @click="void preview()">
          {{ busy ? t("compile.previewing") : t("compile.preview") }}
        </button>
        <ul v-if="files.length > 0" class="compile__files">
          <li v-for="file in files" :key="file.path" class="compile__file">
            {{ t("compile.file", { path: file.path, size: fileSize(file.bytes) }) }}
          </li>
        </ul>
      </div>

      <footer class="compile__foot">
        <span class="compile__note">{{ note }}</span>
        <button
          v-if="files.length > 0"
          type="button"
          class="dialog__button"
          :title="t('compile.open_folder_title')"
          @click="void openFolder()"
        >
          {{ t("compile.open_folder") }}
        </button>
        <button type="button" class="dialog__button compile__run" :disabled="busy" @click="void run()">
          {{ busy ? t("compile.running") : t("compile.run") }}
        </button>
      </footer>
    </section>
  </div>
</template>

<style scoped src="./dialog.css"></style>
<style scoped src="./compile-dialog.css"></style>
