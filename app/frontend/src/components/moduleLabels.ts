//! 模块卡片上的小标签：读权限 / 认证；纯展示，方便单独回归。
import { t } from "../locales/index.ts";
import type { ModuleCard } from "../api/modules";

export function readsLabel(card: ModuleCard): string {
  if (card.reads.length === 0) return t("modules.no_reads");
  return card.reads
    .map((code) => t(`modules.read.${code}`))
    .join(t("common.list_separator"));
}

export function certificationLabel(card: ModuleCard): string {
  if (!card.certified) return t("modules.uncertified");
  const date = new Date(card.certified_at).toISOString().slice(0, 10);
  return t("modules.certified_badge", { edition: card.cert_edition, date });
}
