import { describe, expect, it } from "vitest";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import { newArrivals, notificationKeyId } from "@/utils/notificationArrivals";

function welcome(): NotificationDto {
  return {
    key: { kind: "welcome", fingerprint: "welcome" },
    severity: "info",
    actions: ["keepNetworkSettings", "turnOffNetwork", "applyRecommendedSettings"],
    dismissal: "occurrence",
    data: {
      kind: "welcome",
      network: {
        allowNetwork: true,
        checkForUpdates: true,
        autoRefreshRuleDatabases: true,
        fetchCommunityRules: true,
        fetchSteamWorkshop: true,
        fetchRimmergeRules: true,
      },
      settingsMatchRecommended: true,
    },
  };
}

function updateAvailable(version = "0.2.0"): NotificationDto {
  return {
    key: { kind: "updateAvailable", fingerprint: version },
    severity: "info",
    actions: ["showReleasePage", "openSettings"],
    dismissal: "occurrence",
    data: {
      kind: "updateAvailable",
      latestVersion: version,
      running: "0.1.0",
      latestPublishedAt: "2026-01-01T00:00:00Z",
    },
  };
}

describe("notificationKeyId", () => {
  it("combines the kind and fingerprint", () => {
    expect(notificationKeyId(welcome())).toBe("welcome:welcome");
    expect(notificationKeyId(updateAvailable("1.0.0"))).toBe("updateAvailable:1.0.0");
  });
});

describe("newArrivals", () => {
  it("returns nothing on the very first load, even though every notice is technically new", () => {
    expect(newArrivals(null, [welcome(), updateAvailable()])).toEqual([]);
  });

  it("returns nothing when the set is unchanged", () => {
    const list = [welcome()];
    const previousKeys = new Set(list.map(notificationKeyId));
    expect(newArrivals(previousKeys, list)).toEqual([]);
  });

  it("returns nothing on a decrease (a dismiss/mute)", () => {
    const previousKeys = new Set([
      notificationKeyId(welcome()),
      notificationKeyId(updateAvailable()),
    ]);
    expect(newArrivals(previousKeys, [welcome()])).toEqual([]);
  });

  it("returns exactly the notices absent from the previous set", () => {
    const previousKeys = new Set([notificationKeyId(welcome())]);
    const arrival = updateAvailable();
    expect(newArrivals(previousKeys, [welcome(), arrival])).toEqual([arrival]);
  });

  it("returns every simultaneous arrival, not just one", () => {
    const arrivals = [updateAvailable("1.0.0"), updateAvailable("2.0.0")];
    expect(newArrivals(new Set(), arrivals)).toEqual(arrivals);
  });
});
