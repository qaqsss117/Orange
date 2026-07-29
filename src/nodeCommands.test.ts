import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import publicFixture from "../contracts/business-api/fixtures/public-success.v1.json";
import {
  COMMANDS,
  getNodeCatalog,
  getSubscriptionSnapshot,
  parseNodeCatalogResponse,
  selectNode,
  testNodeDelays,
} from "./ipc";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const catalog = {
  schemaVersion: 2,
  revision: 1_785_157_200_000,
  groups: [
    {
      id: "proxy",
      selectedNodeId: "node-01",
      nodes: [
        { id: "node-01", protocol: "vless" },
        { id: "node-02", protocol: "vless" },
      ],
    },
  ],
} as const;

describe("subscription and public node commands", () => {
  beforeEach(() => invokeMock.mockReset());
  afterEach(() => vi.restoreAllMocks());

  it("uses closed v2 requests and parses credential-free responses", async () => {
    invokeMock
      .mockResolvedValueOnce({
        schemaVersion: 2,
        subscription: publicFixture.responses.subscription,
        localRevision: 1_785_157_200_000,
      })
      .mockResolvedValueOnce(catalog)
      .mockResolvedValueOnce({
        schemaVersion: 2,
        selectorId: "proxy",
        nodeId: "node-02",
      })
      .mockResolvedValueOnce({
        schemaVersion: 2,
        results: [
          {
            selectorId: "proxy",
            nodeId: "node-02",
            result: { status: "available", delayMs: 38 },
          },
        ],
      });

    await getSubscriptionSnapshot();
    await getNodeCatalog();
    await selectNode("proxy", "node-02");
    await testNodeDelays();

    expect(invokeMock.mock.calls).toEqual([
      [COMMANDS.getSubscriptionSnapshot, { request: { schemaVersion: 2 } }],
      [COMMANDS.getNodeCatalog, { request: { schemaVersion: 2 } }],
      [
        COMMANDS.selectNode,
        {
          request: {
            schemaVersion: 2,
            selectorId: "proxy",
            nodeId: "node-02",
          },
        },
      ],
      [COMMANDS.testNodeDelays, { request: { schemaVersion: 2 } }],
    ]);
  });

  it("rejects invalid node identities before invoking native code", async () => {
    await expect(selectNode("proxy", "../private-key")).rejects.toThrow(
      "PublicNodeId contract violation",
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("rejects catalogs with an unknown selected node", () => {
    expect(() =>
      parseNodeCatalogResponse({
        ...catalog,
        groups: [{ ...catalog.groups[0], selectedNodeId: "node-99" }],
      }),
    ).toThrow("NodeCatalogResponse contract violation");
  });
});
