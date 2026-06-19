import { describe, expect, it } from "vitest";
import { fetchAllActiveEntities, fetchRegistryEntityById, fetchUserEntityIds } from "@/earnings/entityRegistryQueries";
import type { EntityRegistryApi } from "@/earnings/entityRegistryQueries";

describe("earnings/entityRegistryQueries", () => {
  it("fetchAllActiveEntities filters inactive and sorts by name", async () => {
    const api = {
      query: {
        entityRegistry: {
          entities: {
            entries: async () => [
              [
                2,
                {
                  isNone: false,
                  unwrap: () => ({
                    toJSON: () => ({
                      id: 2,
                      name: [90, 101, 116, 97],
                      status: "Suspended",
                      primaryShopId: 20,
                    }),
                  }),
                },
              ],
              [
                1,
                {
                  isNone: false,
                  unwrap: () => ({
                    toJSON: () => ({
                      id: 1,
                      name: "Alpha",
                      status: "Active",
                      primaryShopId: 10,
                      verified: true,
                      entityType: "Merchant",
                    }),
                  }),
                },
              ],
            ],
          },
        },
      },
    } as unknown as EntityRegistryApi;

    const list = await fetchAllActiveEntities(api);
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({
      id: 1,
      name: "Alpha",
      primaryShopId: 10,
      verified: true,
      entityType: "Merchant",
    });
  });

  it("fetchRegistryEntityById returns entity regardless of status", async () => {
    const api = {
      query: {
        entityRegistry: {
          entities: async (id: number) => ({
            isNone: false,
            unwrap: () => ({
              toJSON: () => ({
                id,
                name: "PausedCo",
                status: "Suspended",
                primaryShopId: 99,
              }),
            }),
          }),
        },
      },
    } as unknown as EntityRegistryApi;

    const reg = await fetchRegistryEntityById(api, 100010);
    expect(reg).toMatchObject({ id: 100010, name: "PausedCo", status: "Suspended" });
  });

  it("fetchUserEntityIds reads userEntity storage", async () => {
    const api = {
      query: {
        entityRegistry: {
          userEntity: async () => [1, 2, 3],
        },
      },
    } as unknown as EntityRegistryApi;
    const ids = await fetchUserEntityIds(api, "5Alice");
    expect(ids).toEqual([1, 2, 3]);
  });

  it("decodes hex-encoded entity name from toJSON", async () => {
    const api = {
      query: {
        entityRegistry: {
          entities: async () => ({
            isNone: false,
            unwrap: () => ({
              toJSON: () => ({
                id: 100010,
                name: "0x4e6578757320436f6d6d756e697479",
                status: "Active",
                primaryShopId: 11,
              }),
            }),
          }),
        },
      },
    } as unknown as EntityRegistryApi;

    const reg = await fetchRegistryEntityById(api, 100010);
    expect(reg?.name).toBe("Nexus Community");
  });
});
