// 加密备份包的钥匙与开关：**类型与薄封装**（命令名仍在 `./core` 的白名单里统一登记）。
//
// 为什么单开一个文件：与 `mirror.ts` / `volumes.ts` 同一条口径——
// **命令名进白名单（`core.ts`），类型与调用放各自域**，通道仍然只有一条
//（`core.ts` 是唯一 import Tauri API 的地方）。
//
// 三条分寸写在最前面（界面这一层最容易搞错的三处）：
// - **口令只在内存**：`unlockSeal` 只是把口令交上去记住，**这里验不了对错**——
//   对不对要等真去开一个包才知道（那时报「打不开」）；
// - **恢复码只在开启时给一次**：本机已经有一把就不再往外吐新的（换掉它会让
//   以前那些包失去第二把钥匙），所以 `enableSeal` 的返回要分"有码 / 没码"两种；
// - **加密包不能走 preview**：清单在密文里，壳会明确拒绝（`backup.restore_sealed_needs_key`）——
//   要先给钥匙（`unlockRestoreSource` 解开到临时目录 + 体检），再拿解开的目录当普通来源。

import { call, COMMANDS, type RestorePreview } from "./core";

/** 加密那一格的现状（**不含任何钥匙本身**）。 */
export interface SealStatus {
  /** 作者要不要加密（写出去的备份包要不要口令才打得开） */
  enabled: boolean;
  /** 此刻内存里有没有口令——**重启之后是 false**，界面要如实说 */
  unlocked: boolean;
  /** 本机有没有记着一把恢复码 */
  has_recovery: boolean;
}

/** 读一眼现状（读的是壳里那份内存状态，很便宜）。 */
export const readSealStatus = () => call<SealStatus>(COMMANDS.sealStatus);

/**
 * 开启加密：口令进内存 + 打开开关。
 *
 * **只有本机还没有过恢复码时才返回那一串**——界面那时必须让作者抄下来（包里取不回来）；
 * 已经有一把就返回 `null`（这时界面说一句"本机已有一把"即可）。口令为空会被壳拒收
 *（`backup.seal_passphrase_empty`）。
 */
export const enableSeal = (passphrase: string) =>
  call<string | null>(COMMANDS.sealEnable, { passphrase });

/** 关掉加密：**只关开关**——恢复码与本机记着的钥匙都不动（以前那些包还能开）。 */
export const disableSeal = () => call<SealStatus>(COMMANDS.sealDisable);

/** 给口令（放内存）。**这里验不了对错**：只有真去解一个包才知道它对不对。 */
export const unlockSeal = (passphrase: string) =>
  call<SealStatus>(COMMANDS.sealUnlock, { passphrase });

/** 把口令从内存里撤掉（"锁上"）。**不动开关**：作者仍要加密，只是这一会儿没给口令。 */
export const lockSeal = () => call<void>(COMMANDS.sealLock);

/** 解开一个加密包：`source` 是**解开之后的目录**（点「换库」时把它当普通来源递回去）。 */
export interface UnsealedSource {
  preview: RestorePreview;
  source: string;
}

/**
 * 给一个加密包的钥匙：`kind` 两档 `passphrase`（作者记的那句话）/ `recovery`（抄在纸上那一串）。
 *
 * 两把都是同一份数据的钥匙，**任一把都行**；给错了只说"打不开"（不泄露是哪一步错的）。
 * 解开之后顺手体检，一次把结论与解开的目录都给回来。
 */
export const unlockRestoreSource = (source: string, kind: "passphrase" | "recovery", secret: string) =>
  call<UnsealedSource>(COMMANDS.backupRestoreUnlock, { source, kind, secret });
