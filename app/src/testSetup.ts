// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

import i18n from "./i18n";

localStorage.removeItem("hamheatmap.locale.v1");
await i18n.changeLanguage("zh-CN");
