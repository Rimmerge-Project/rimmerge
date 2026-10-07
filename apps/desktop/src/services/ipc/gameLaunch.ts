// Starting RimWorld: the Launch RimWorld button's status and its click.

import type { GameLaunchedDto } from "@/types/generated/GameLaunchedDto";
import type { GameLaunchStatusDto } from "@/types/generated/GameLaunchStatusDto";
import type { LaunchGameRequestDto } from "@/types/generated/LaunchGameRequestDto";
import { call } from "./core";

/**
 * What the Launch RimWorld button shows right now. Read-only; it can wait
 * behind a long command (a verify) that holds the session lock.
 */
export function getGameLaunchStatus(): Promise<GameLaunchStatusDto> {
  return call("get_game_launch_status");
}

/**
 * Starts RimWorld. The backend re-derives the status first and refuses a
 * running game, an unavailable install, and (under `refuse`) an order not yet
 * in `ModsConfig.xml`. Writes nothing.
 */
export function launchGame(request: LaunchGameRequestDto): Promise<GameLaunchedDto> {
  return call("launch_game", { request });
}
