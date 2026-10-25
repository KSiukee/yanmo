<script setup lang="ts">
// 导入预览：**只展示事实 + 逐项同意**。不判断正版，也不执行任何东西。
//
// 单独成件：它跟"模块列表"的变化理由不同（权限清单会长、协议提示会变），
// 留在管理弹窗里会把那个文件顶成上帝类。
import { ref } from "vue";

import type { ImportPreview, ModuleCapabilities } from "../api/modules";
import { t } from "../locales/index.ts";

const emit = defineEmits<{
  confirm: [grant: ModuleCapabilities];
  cancel: [];
}>();

// 升级时**预勾选已有的授予**：用户不必重勾一遍，而新声明出来的能力默认还是不给。
const props = defineProps<{
  preview: ImportPreview;
  busy: boolean;
}>();
const grant = ref<ModuleCapabilities>({
  read: [...props.preview.granted.read],
  own_data: props.preview.granted.own_data,
  cross_read: [...props.preview.granted.cross_read],
  ask_questions: props.preview.granted.ask_questions,
  network: props.preview.granted.network,
});

/** 新增能力码 → 人话（`read.body` 借已有的权限标签）。 */
function addedLabel(code: string): string {
  if (code.startsWith("read.")) return t(`modules.grant.read.${code.slice(5)}`);
  if (code === "own_data") return t("modules.grant.own_data");
  if (code === "network") return t("modules.grant.network");
  if (code === "ask_questions") return t("modules.grant.ask_questions");
  if (code.startsWith("cross_read.")) return `${t("modules.grant.cross_read")} ${code.slice(11)}`;
  return code;
}
</script>

<template>
  <section class="module-import">
    <h3 class="module-import__title">{{ t("modules.import_preview") }}</h3>
    <p class="module-import__meta">
      {{ preview.name }} v{{ preview.version }} · {{ preview.author }} · {{ preview.module_id }}
    </p>
    <p class="module-import__meta">
      {{
        t("modules.import_fingerprint", {
          fingerprint: `${preview.fingerprint.slice(0, 16)}…`,
        })
      }}
    </p>
    <p class="module-import__meta">
      {{
        preview.certified
          ? t("modules.certified_badge", {
              edition: preview.cert_edition,
              date: new Date(preview.certified_at).toISOString().slice(0, 10),
            })
          : t("modules.uncertified")
      }}
    </p>
    <p class="module-import__hint">{{ t("modules.cert_verify_hint") }}</p>
    <p v-if="!preview.compatible" class="module-import__protocol">
      {{
        t("modules.import_protocol_bad", {
          actual: preview.protocol,
          expected: preview.expected_protocol,
        })
      }}
    </p>
    <p v-if="preview.upgrade" class="module-import__upgrade">
      {{ t("modules.import_upgrade", { from: preview.installed_version, to: preview.version }) }}
    </p>
    <p v-if="preview.installed && !preview.upgrade" class="module-import__protocol">
      {{
        t("modules.import_same_or_older", {
          current: preview.installed_version,
          found: preview.version,
        })
      }}
    </p>
    <p v-if="preview.upgrade" class="module-import__hint">
      {{
        preview.added.length > 0
          ? t("modules.import_added", {
              list: preview.added.map((code) => addedLabel(code)).join(t("common.list_separator")),
            })
          : t("modules.import_added_none")
      }}
    </p>
    <p class="module-import__declared">{{ t("modules.declared") }}</p>
    <div class="module-import__grants">
      <label v-for="area in preview.declared.read" :key="area" class="module-import__grant">
        <input v-model="grant.read" type="checkbox" :value="area" />
        {{ t(`modules.grant.read.${area}`) }}
      </label>
      <label v-if="preview.declared.own_data" class="module-import__grant">
        <input v-model="grant.own_data" type="checkbox" />
        {{ t("modules.grant.own_data") }}
      </label>
      <label v-if="preview.declared.ask_questions" class="module-import__grant">
        <input v-model="grant.ask_questions" type="checkbox" />
        {{ t("modules.grant.ask_questions") }}
      </label>
      <label v-if="preview.declared.network" class="module-import__grant">
        <input v-model="grant.network" type="checkbox" />
        {{ t("modules.grant.network") }}
      </label>
      <label v-for="id in preview.declared.cross_read" :key="id" class="module-import__grant">
        <input v-model="grant.cross_read" type="checkbox" :value="id" />
        {{ t("modules.grant.cross_read") }} {{ id }}
      </label>
    </div>
    <p class="module-import__hint">{{ t("modules.import_no_grant") }}</p>
    <div class="module-import__actions">
      <button
        type="button"
        class="dialog__button"
        :disabled="busy || !preview.compatible || preview.downgrade"
        @click="emit('confirm', grant)"
      >
        {{ preview.upgrade ? t("modules.import_upgrade_confirm") : t("modules.import_confirm") }}
      </button>
      <button type="button" class="dialog__button" :disabled="busy" @click="emit('cancel')">
        {{ t("modules.import_cancel") }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.module-import {
  margin-bottom: 10px;
  padding: 10px 12px;
  border: 1px solid var(--ym-line);
  border-radius: 6px;
  background: var(--ym-paper-dim);
}

.module-import__title {
  margin: 0 0 6px;
  font-size: 12px;
  font-weight: 600;
}

.module-import__meta,
.module-import__declared {
  margin: 4px 0 0;
  font-size: 12px;
}

.module-import__meta {
  color: var(--ym-ink-soft);
  overflow-wrap: anywhere;
}

.module-import__declared {
  margin-top: 8px;
  font-weight: 600;
}

.module-import__upgrade {
  margin: 6px 0 0;
  font-size: 12px;
  font-weight: 600;
}

.module-import__protocol {
  margin: 6px 0 0;
  color: var(--ym-danger);
  font-size: 12px;
}

.module-import__grants {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.module-import__grant {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
}

.module-import__hint {
  margin: 8px 0 0;
  color: var(--ym-ink-soft);
  font-size: 11px;
}

.module-import__actions {
  display: flex;
  gap: 6px;
  margin-top: 8px;
}
</style>
