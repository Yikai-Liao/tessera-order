// SPDX-License-Identifier: BSD-2-Clause

import { z } from "zod";

/** Linux taskset CPU lists without strides; shared by the CLI and retained sessions. */
export const cpuListSchema = z
  .string()
  .regex(/^\d+(?:-\d+)?(?:,\d+(?:-\d+)?)*$/)
  .refine((value) =>
    value.split(",").every((part) => {
      const [first, last = first] = part.split("-").map(Number);
      return Number.isSafeInteger(first) && Number.isSafeInteger(last) && first <= last;
    }),
  );
