// SPDX-License-Identifier: BSD-2-Clause

import { getOctokit } from "@actions/github";

export interface Request {
  method: string;
  path: string;
  body: Record<string, unknown>;
}

export function mockGitHub(respond: (request: Request) => unknown) {
  const requests: Request[] = [];
  const fetchMock: typeof globalThis.fetch = async (url, options) => {
    const request: Request = {
      method: options?.method ?? "GET",
      path: new URL(String(url)).pathname,
      body: typeof options?.body === "string" ? JSON.parse(options.body) : {},
    };
    requests.push(request);
    const result = respond(request);
    const response =
      result instanceof Response
        ? result
        : new Response(JSON.stringify(result), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
    Object.defineProperty(response, "url", { value: String(url) });
    return response;
  };
  const github = getOctokit("fixture-token", {
    request: { fetch: fetchMock },
  });
  return { github, requests };
}
