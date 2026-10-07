//! What the "Launch RimWorld" button shows, derived from three facts the
//! interface gathers: whether the game is running, how this install can be
//! started, and whether `ModsConfig.xml` holds the selected order.
//!
//! Pure: no IO, no ports. The facts come from [`crate::Session::order_on_disk`],
//! [`crate::ports::GameLauncher::route`], and the process probe `rim-io`
//! owns; [`game_launch_status`] only decides what they add up to. "Launch"
//! here always means starting the *game*; Rimmerge's own start is
//! `RunLaunchNetworkChecks`'s concern.

/// What the Launch RimWorld button shows. Closed; each variant carries only
/// its own fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameLaunchStatus {
    /// A click launches now.
    Ready {
        /// How the game would be started.
        route: LaunchRouteKind,
    },
    /// A click asks "Apply first?" before launching.
    NeedsApply {
        /// How the game would be started.
        route: LaunchRouteKind,
        /// Why `ModsConfig.xml` is behind what the user is looking at.
        reason: UnappliedReason,
    },
    /// `RimWorldWin64.exe` is in the process list. A click is not offered.
    GameRunning,
    /// This install can't be started from here.
    Unavailable(LaunchUnavailable),
}

/// How an install is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchRouteKind {
    /// Through Steam (`steam://run/<app id>`).
    Steam,
    /// By running the install's own `RimWorldWin64.exe`.
    Executable,
}

/// Why `ModsConfig.xml` doesn't hold what the user is looking at (as of the
/// last scan or apply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnappliedReason {
    /// Activations or deactivations staged in the app that no rescan has
    /// picked up ([`crate::Session::is_stale`]).
    ActivationChangesNotScanned,
    /// The selected order differs from the file's last-known list.
    OrderDiffers,
}

/// Why an install can't be started from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchUnavailable {
    /// The game folder isn't an install, or has no regular-file
    /// `RimWorldWin64.exe` (the executable route only).
    ExecutableMissing,
}

/// Whether the game is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameProcess {
    /// `RimWorldWin64.exe` is in the process list.
    Running,
    /// It is not.
    NotRunning,
}

/// Whether `ModsConfig.xml` holds the selected order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderOnDisk {
    /// The file holds the selected order.
    Applied,
    /// It does not.
    NotApplied(UnappliedReason),
}

/// The button's state, first match wins: a running game, then an install
/// that can't be started, then an order the file doesn't hold, else ready.
///
/// A running game outranks everything because that is what the user sees as
/// true, and an unavailable install can't be running from this folder anyway.
#[must_use]
pub fn game_launch_status(
    game: GameProcess,
    route: Result<LaunchRouteKind, LaunchUnavailable>,
    order: OrderOnDisk,
) -> GameLaunchStatus {
    match game {
        GameProcess::Running => return GameLaunchStatus::GameRunning,
        GameProcess::NotRunning => {}
    }
    let route = match route {
        Ok(route) => route,
        Err(reason) => return GameLaunchStatus::Unavailable(reason),
    };
    match order {
        OrderOnDisk::NotApplied(reason) => GameLaunchStatus::NeedsApply { route, reason },
        OrderOnDisk::Applied => GameLaunchStatus::Ready { route },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTES: [LaunchRouteKind; 2] = [LaunchRouteKind::Steam, LaunchRouteKind::Executable];
    const ORDERS: [OrderOnDisk; 3] = [
        OrderOnDisk::Applied,
        OrderOnDisk::NotApplied(UnappliedReason::OrderDiffers),
        OrderOnDisk::NotApplied(UnappliedReason::ActivationChangesNotScanned),
    ];

    #[test]
    fn a_running_game_outranks_every_other_fact() {
        let mut routes: Vec<Result<LaunchRouteKind, LaunchUnavailable>> =
            ROUTES.iter().copied().map(Ok).collect();
        routes.push(Err(LaunchUnavailable::ExecutableMissing));

        for route in routes {
            for order in ORDERS {
                let status = game_launch_status(GameProcess::Running, route, order);

                assert_eq!(status, GameLaunchStatus::GameRunning, "{route:?} {order:?}");
            }
        }
    }

    #[test]
    fn an_unavailable_install_outranks_an_unapplied_order() {
        for order in ORDERS {
            let status = game_launch_status(
                GameProcess::NotRunning,
                Err(LaunchUnavailable::ExecutableMissing),
                order,
            );

            assert_eq!(
                status,
                GameLaunchStatus::Unavailable(LaunchUnavailable::ExecutableMissing)
            );
        }
    }

    #[test]
    fn an_unapplied_order_needs_apply_with_its_reason() {
        for route in ROUTES {
            for reason in [
                UnappliedReason::OrderDiffers,
                UnappliedReason::ActivationChangesNotScanned,
            ] {
                let status = game_launch_status(
                    GameProcess::NotRunning,
                    Ok(route),
                    OrderOnDisk::NotApplied(reason),
                );

                assert_eq!(status, GameLaunchStatus::NeedsApply { route, reason });
            }
        }
    }

    #[test]
    fn applied_with_a_route_is_ready() {
        for route in ROUTES {
            let status =
                game_launch_status(GameProcess::NotRunning, Ok(route), OrderOnDisk::Applied);

            assert_eq!(status, GameLaunchStatus::Ready { route });
        }
    }
}
