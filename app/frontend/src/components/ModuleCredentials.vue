<script setup lang="ts">
//! 模块凭据：密钥只写进系统钥匙串；这里只看得到名字与允许域名。
import { onMounted, ref } from "vue";

import {
  credentialAdd,
  credentialRemove,
  credentialsList,
  networkAuditRecent,
  type CredentialBinding,
  type NetworkAuditRow,
} from "../api/modules";
import { t } from "../locales/index.ts";

const props = defineProps<{ moduleId: string }>();
const emit = defineEmits<{ close: [] }>();

const bindings = ref<CredentialBinding[]>([]);
const name = ref("");
const host = ref("");
const secret = ref("");
const busy = ref(false);
const note = ref("");
const audit = ref<NetworkAuditRow[]>([]);

async function load() {
  try {
    bindings.value = await credentialsList(props.moduleId);
    audit.value = await networkAuditRecent(20);
  } catch (error) {
    note.value = t("credentials.load_failed", { detail: String(error) });
  }
}

async function add() {
  if (busy.value) return;
  if (!name.value.trim() || !host.value.trim() || !secret.value) {
    note.value = t("credentials.need_all");
    return;
  }
  busy.value = true;
  note.value = "";
  try {
    await credentialAdd(
      props.moduleId,
      name.value.trim(),
      host.value.trim(),
      secret.value,
    );
    secret.value = "";
    name.value = "";
    note.value = t("credentials.added");
    await load();
  } catch (error) {
    note.value = t("credentials.save_failed", { detail: String(error) });
  } finally {
    busy.value = false;
  }
}

async function remove(item: CredentialBinding) {
  if (!window.confirm(t("credentials.remove_confirm", { name: item.name }))) return;
  busy.value = true;
  try {
    await credentialRemove(props.moduleId, item.name);
    note.value = t("credentials.removed");
    await load();
  } catch (error) {
    note.value = t("credentials.save_failed", { detail: String(error) });
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section class="credentials">
    <header class="credentials__head">
      <strong>{{ t("credentials.title") }}</strong>
      <button type="button" class="credentials__link" @click="emit('close')">
        {{ t("common.close") }}
      </button>
    </header>
    <p class="credentials__hint">{{ t("credentials.hint") }}</p>
    <ul v-if="bindings.length" class="credentials__list">
      <li v-for="item in bindings" :key="item.name" class="credentials__row">
        <span>{{ item.name }} → {{ item.host }}</span>
        <button
          type="button"
          class="credentials__link"
          :disabled="busy"
          @click="remove(item)"
        >
          {{ t("credentials.remove") }}
        </button>
      </li>
    </ul>
    <p v-else class="credentials__hint">{{ t("credentials.empty") }}</p>
    <div class="credentials__form">
      <input v-model="name" :placeholder="t('credentials.name')" :disabled="busy" />
      <input v-model="host" :placeholder="t('credentials.host')" :disabled="busy" />
      <input
        v-model="secret"
        type="password"
        :placeholder="t('credentials.secret')"
        :disabled="busy"
        autocomplete="off"
      />
      <button type="button" class="credentials__button" :disabled="busy" @click="add">
        {{ t("credentials.add") }}
      </button>
    </div>
    <p v-if="note" class="credentials__note">{{ note }}</p>
    <p class="credentials__hint">{{ t("credentials.audit_hint") }}</p>
    <ul v-if="audit.length" class="credentials__audit">
      <li v-for="(row, index) in audit" :key="index">
        {{ new Date(row.ts).toLocaleString() }} · {{ row.module }} → {{ row.host }} ·
        {{ row.bytes }} 字节 · {{ row.credential ?? "-" }} · {{ row.result }}
      </li>
    </ul>
    <p v-else class="credentials__hint">{{ t("credentials.audit_empty") }}</p>
  </section>
</template>

<style scoped>
.credentials {
  margin-top: 10px;
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel-soft);
}
.credentials__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.credentials__hint,
.credentials__note {
  margin: 6px 0;
  font-size: 12px;
  color: var(--muted);
}
.credentials__list,
.credentials__audit {
  margin: 6px 0;
  padding: 0;
  list-style: none;
}
.credentials__audit li {
  padding: 2px 0;
  font-size: 11px;
  color: var(--muted);
  word-break: break-all;
}
.credentials__row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding: 3px 0;
  font-size: 12px;
}
.credentials__form {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 6px;
}
.credentials__form input {
  flex: 1 1 120px;
  min-width: 0;
  padding: 4px 6px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--panel);
  color: var(--text);
}
.credentials__button,
.credentials__link {
  padding: 4px 8px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--panel);
  color: var(--text);
  cursor: pointer;
  font-size: 12px;
}
.credentials__link {
  border-color: transparent;
  background: transparent;
  color: var(--muted);
}
</style>
