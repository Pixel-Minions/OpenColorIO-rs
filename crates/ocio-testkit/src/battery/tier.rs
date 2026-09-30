// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The battery's tiers (card T3d): how much a run probes, chosen with `OCIO_RS_TIER`.
//!
//! | Tier | When | Per explicit case and combination | Generated cases |
//! |---|---|---|---|
//! | `quick` (default) | every chunk | every 61st half (1,075 values), the specials (170), 256 random values of each of the 9 ranges (2,304), ±3 ulp around the break points, NaN buffers of 1 to 24 pixels (48 buffers, 600 pixels) | 2 per parameter slot, on the same probes |
//! | `full` | every land | all 65,536 halves, the specials, 1,200,000 random values (the S2 tests' 1,000,000, then negative and huge), ±8 ulp, the NaN buffers | every slot with every value, on the quick probes |
//! | `exhaustive` | nightly | the full probes, 9,999,999 more random values, ±64 ulp, NaN buffers of 1 to 64 pixels (128 buffers, 4,160 pixels), no cache; the first explicit case also sweeps every `f32` bit pattern ([`Sweep`]) | every slot with every value, on the full probes |
//!
//! Every tier runs every explicit case, in both directions (or the family's), with fast math
//! on and off.
//!
//! **Costs** of the Log and Gamma families (`crates/ocio-ops/tests/log_oracle.rs` and
//! `gamma_oracle.rs`, 11 tests; before the battery they made 308 comparisons of 903.7 million
//! values in 308 oracle processes), measured on the development machine (Ryzen 9 9950X3D),
//! wall time of both test binaries with cargo's parallel tests, without the oracle cache
//! (cold) and with it (warm):
//!
//! | Tier | Cases (generated) | Comparisons | Values | Oracle processes | Windows debug | Rocky debug | Windows release | Rocky release |
//! |---|---|---|---|---|---|---|---|---|
//! | before | 54 (0) | 308 | 903.7 M | 308 | 37.0 s / 4.0 s | 57.3 s / 4.3 s | 41.8 s / 3.7 s | 58.7 s / 4.2 s |
//! | `quick` | 176 (122) | 26,240 | 25.5 M | 13 | 4.2 s / 0.9 s | 3.7 s / 0.9 s | 4.6 s / 1.0 s | 3.7 s / 1.0 s |
//! | `full` | 725 (671) | 82,436 | 1,093.9 M | 27 | 24.2 s / 5.8 s | 19.2 s / 6.4 s | 23.8 s / 5.4 s | 20.4 s / 5.0 s |
//!
//! Comparisons are pixel buffers; the integer-cast test (4 buffers, 17.1 M values) runs the
//! S2 probe values at every tier. Generated cases that the wheel refuses (while a family's
//! validation isn't ported) are counted in the cases but not compared. At `full`, the explicit
//! cases see every value the old tests saw: all halves, the specials, the S2 random values,
//! and around each break point every bit pattern within 3 steps (the neighbourhoods hold both
//! the value-order and the bit-pattern neighbours, so the old neighbours of zero, 0xfffffffd
//! to 0xffffffff, are there). A family run with many generated cases makes
//! tens of thousands of small calls: about 0.13 ms each inside one oracle process, against
//! 0.3 s to start a process natively (about 10 s under Intel SDE).
//!
//! `exhaustive` adds minutes per family, plus the sweep: 2^30 pixels per combination in 256
//! calls, 64 processes, about 5.5 minutes per swept case (four combinations) in a release build
//! and 6.5 minutes in debug, nearly all of it in the oracle and the pipes. A running sweep
//! holds several copies of its 256 MiB batch on each side of the pipe; run the tier with
//! `--test-threads=2` or fewer on machines with 16 GB or less.

use super::{Mutations, Plan, Sweep};
use crate::probe::{ProbeSet, RandomRange};

/// How thoroughly the battery probes. See the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// Every chunk: sampled probes; seconds per family.
    Quick,
    /// Every land: all halves, the specials and 1.2 million random values per explicit case,
    /// every generated case.
    Full,
    /// Nightly: 10 million more random values per explicit case, generated cases on the full
    /// probes, and the sweep of every `f32` bit pattern.
    Exhaustive,
}

impl Tier {
    /// Every tier, from the quickest.
    pub const ALL: [Tier; 3] = [Tier::Quick, Tier::Full, Tier::Exhaustive];

    /// The tier `OCIO_RS_TIER` names ([`Tier::from_env`]). Panics on any value but `quick`,
    /// `full` or `exhaustive`, so a typo can't quietly run the quick tier.
    pub fn current() -> Tier {
        Tier::from_env(std::env::var_os("OCIO_RS_TIER").as_deref())
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The tier for a value of `OCIO_RS_TIER`: `quick` when it is unset, else the tier it
    /// names. Any other value, empty or not Unicode included, is an error.
    pub fn from_env(value: Option<&std::ffi::OsStr>) -> Result<Tier, String> {
        match value {
            None => Ok(Tier::Quick),
            Some(value) => value.to_str().and_then(Tier::from_name).ok_or_else(|| {
                format!("OCIO_RS_TIER={value:?}: expected quick, full or exhaustive")
            }),
        }
    }

    /// The tier called `name`.
    pub fn from_name(name: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The tier's name: `quick`, `full` or `exhaustive`.
    pub fn name(self) -> &'static str {
        match self {
            Tier::Quick => "quick",
            Tier::Full => "full",
            Tier::Exhaustive => "exhaustive",
        }
    }

    /// The tier's plan.
    pub fn plan(self) -> Plan {
        match self {
            Tier::Quick => quick(),
            Tier::Full => full(),
            Tier::Exhaustive => exhaustive(),
        }
    }
}

/// Every 61st half value, the specials, 256 random values of every range, and NaN buffers.
fn quick_probes() -> Vec<ProbeSet> {
    vec![
        ProbeSet::Halves { stride: 61 },
        ProbeSet::Specials,
        ProbeSet::Random {
            name: "of every range",
            seed: 0x0b47_7e27_0001,
            ranges: RandomRange::ALL.iter().map(|&r| (r, 256)).collect(),
        },
        ProbeSet::NanBuffers { max_pixels: 24 },
    ]
}

/// All halves, the specials, 1,200,000 random values and NaN buffers. The first two random
/// streams are the S2 oracle tests' 1,000,000 probe values, value for value.
fn full_probes() -> Vec<ProbeSet> {
    use RandomRange::{AllBits, Exponent, Finite, Hdr, Huge, Negative, Overshoot, Tiny, Unit};
    vec![
        ProbeSet::Halves { stride: 1 },
        ProbeSet::Specials,
        ProbeSet::Random {
            name: "unit, overshoot, exponent, hdr",
            seed: 0x5252_0001,
            ranges: vec![
                (Unit, 200_000),
                (Overshoot, 200_000),
                (Exponent, 100_000),
                (Hdr, 100_000),
            ],
        },
        ProbeSet::Random {
            name: "finite, tiny, all-bits",
            seed: 0x5252_0002,
            ranges: vec![(Finite, 200_000), (Tiny, 100_000), (AllBits, 100_000)],
        },
        ProbeSet::Random {
            name: "negative, huge",
            seed: 0x5252_0003,
            ranges: vec![(Negative, 100_000), (Huge, 100_000)],
        },
        ProbeSet::NanBuffers { max_pixels: 24 },
    ]
}

fn quick() -> Plan {
    Plan {
        name: Tier::Quick.name().to_string(),
        probes: quick_probes(),
        generated_probes: quick_probes(),
        breakpoint_ulps: 3,
        mutations: Mutations::Sampled,
        sweep: None,
        batch_bytes: 256 << 20,
        cache: true,
    }
}

fn full() -> Plan {
    Plan {
        name: Tier::Full.name().to_string(),
        probes: full_probes(),
        generated_probes: quick_probes(),
        breakpoint_ulps: 8,
        mutations: Mutations::All,
        sweep: None,
        batch_bytes: 256 << 20,
        cache: true,
    }
}

fn exhaustive() -> Plan {
    let mut probes = full_probes();
    // Replace the 24-pixel NaN buffers with longer ones.
    probes.pop();
    probes.push(ProbeSet::Random {
        name: "ten million of every range",
        seed: 0x5252_0004,
        ranges: RandomRange::ALL.iter().map(|&r| (r, 1_111_111)).collect(),
    });
    probes.push(ProbeSet::NanBuffers { max_pixels: 64 });
    Plan {
        name: Tier::Exhaustive.name().to_string(),
        probes,
        generated_probes: full_probes(),
        breakpoint_ulps: 64,
        mutations: Mutations::All,
        sweep: Some(Sweep {
            cases: 1,
            chunk_pixels: 1 << 22,
            chunks: None,
        }),
        batch_bytes: 256 << 20,
        cache: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_have_names() {
        for tier in Tier::ALL {
            assert_eq!(Tier::from_name(tier.name()), Some(tier));
            assert_eq!(tier.plan().name, tier.name());
        }
        assert_eq!(Tier::from_name("Full"), None);
    }

    /// Unset is quick; a tier's name is that tier; anything else is an error, never quick.
    #[test]
    fn the_environment_names_a_tier_or_fails() {
        use std::ffi::OsStr;
        assert_eq!(Tier::from_env(None), Ok(Tier::Quick));
        for tier in Tier::ALL {
            assert_eq!(Tier::from_env(Some(OsStr::new(tier.name()))), Ok(tier));
        }
        for bad in ["", "Full", "QUICK", " full", "full ", "fast", "exhaustive2"] {
            let error = Tier::from_env(Some(OsStr::new(bad))).expect_err(bad);
            assert!(
                error.contains("expected quick, full or exhaustive"),
                "{error}"
            );
        }
    }

    /// The tiers' probe sets, pinned: changing a tier is a deliberate edit of this test too.
    /// Every tier probes the specials and the NaN buffers for explicit and generated cases.
    #[test]
    fn the_tiers_probe_sets_are_pinned() {
        use RandomRange::{AllBits, Exponent, Finite, Hdr, Huge, Negative, Overshoot, Tiny, Unit};
        let quick = vec![
            ProbeSet::Halves { stride: 61 },
            ProbeSet::Specials,
            ProbeSet::Random {
                name: "of every range",
                seed: 0x0b47_7e27_0001,
                ranges: RandomRange::ALL.iter().map(|&r| (r, 256)).collect(),
            },
            ProbeSet::NanBuffers { max_pixels: 24 },
        ];
        let full = vec![
            ProbeSet::Halves { stride: 1 },
            ProbeSet::Specials,
            ProbeSet::Random {
                name: "unit, overshoot, exponent, hdr",
                seed: 0x5252_0001,
                ranges: vec![
                    (Unit, 200_000),
                    (Overshoot, 200_000),
                    (Exponent, 100_000),
                    (Hdr, 100_000),
                ],
            },
            ProbeSet::Random {
                name: "finite, tiny, all-bits",
                seed: 0x5252_0002,
                ranges: vec![(Finite, 200_000), (Tiny, 100_000), (AllBits, 100_000)],
            },
            ProbeSet::Random {
                name: "negative, huge",
                seed: 0x5252_0003,
                ranges: vec![(Negative, 100_000), (Huge, 100_000)],
            },
            ProbeSet::NanBuffers { max_pixels: 24 },
        ];
        let mut exhaustive = full[..5].to_vec();
        exhaustive.push(ProbeSet::Random {
            name: "ten million of every range",
            seed: 0x5252_0004,
            ranges: RandomRange::ALL.iter().map(|&r| (r, 1_111_111)).collect(),
        });
        exhaustive.push(ProbeSet::NanBuffers { max_pixels: 64 });

        let plans = Tier::ALL.map(Tier::plan);
        assert_eq!(plans[0].probes, quick);
        assert_eq!(plans[0].generated_probes, quick);
        assert_eq!(plans[1].probes, full);
        assert_eq!(plans[1].generated_probes, quick);
        assert_eq!(plans[2].probes, exhaustive);
        assert_eq!(plans[2].generated_probes, full);
        for plan in &plans {
            for sets in [&plan.probes, &plan.generated_probes] {
                assert!(sets.contains(&ProbeSet::Specials), "{}", plan.name);
                assert!(
                    sets.iter()
                        .any(|s| matches!(s, ProbeSet::NanBuffers { .. })),
                    "{}",
                    plan.name
                );
            }
        }
    }

    /// Each tier probes at least what the one below it probes.
    #[test]
    fn tiers_grow() {
        let values = |sets: &[ProbeSet]| -> usize {
            sets.iter()
                .flat_map(ProbeSet::rgba_buffers)
                .map(|(_, px)| px.len() / 4)
                .sum()
        };
        let plans = Tier::ALL.map(Tier::plan);
        for pair in plans.windows(2) {
            let (lower, higher) = (&pair[0], &pair[1]);
            assert!(values(&higher.probes) > values(&lower.probes));
            assert!(values(&higher.generated_probes) >= values(&lower.generated_probes));
            assert!(higher.breakpoint_ulps > lower.breakpoint_ulps);
        }
        assert_eq!(plans[0].mutations, Mutations::Sampled);
        assert_eq!(plans[1].mutations, Mutations::All);
        assert!(plans[2].sweep.is_some() && !plans[2].cache);
    }

    /// The full tier holds the S2 oracle tests' probe values: all halves, the specials and
    /// their 1,000,000 random values, value for value (`probe::random_streams_share_one_generator`
    /// checks the streams).
    #[test]
    fn the_full_tier_holds_the_s2_probe_values() {
        let full = Tier::Full.plan();
        let names: Vec<String> = full.probes.iter().map(ProbeSet::name).collect();
        assert_eq!(
            names[..4],
            [
                "all halves",
                "specials",
                "random unit, overshoot, exponent, hdr",
                "random finite, tiny, all-bits"
            ]
        );
        let random: usize = full.probes[2..4]
            .iter()
            .map(|set| match set {
                ProbeSet::Random { ranges, .. } => ranges.iter().map(|(_, n)| n).sum(),
                _ => 0,
            })
            .sum();
        assert_eq!(random, 1_000_000);
    }
}
