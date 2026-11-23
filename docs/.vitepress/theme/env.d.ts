// SPDX-License-Identifier: BSD-2-Clause
/// <reference types="vite/client" />

declare module "vitepress/dist/client/app/utils.js" {
  export const contentUpdatedCallbacks: Array<() => void>;
}
