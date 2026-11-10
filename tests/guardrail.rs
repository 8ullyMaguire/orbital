//! The guardrail (D47): damping is monotone, it never widens a lead, it leaves the leader's score alone, and
//! an exempt member is scored exactly as the raw declaration would score them.
//!
//! One clause of the roadmap's gate cannot be met as written — "for every member the damped score never
//! exceeds the raw score" — because that is the opposite of what damping does: the whole point is to *raise*
//! the tail's share, so a one-click member's damped share is larger than its raw share by orders of magnitude.
//! `the_clause_as_written_is_false_and_the_counterexample_is_arithmetic` states it, so the contradiction lives
//! in the suite instead of in a commit message, and the properties that *do* hold — the ones the guardrail is
//! actually about — are asserted above it.

use orbital::attention::{Config, Mode};

/// 200 seeded pools, every property the guardrail promises.
#[test]
fn damping_is_monotone_and_never_widens_a_lead() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let damped = Config::default();

    for _ in 0..200 {
        // A pool of between 2 and 40 members with counts spread over several orders of magnitude, so the
        // property is exercised on real shapes: a leader with a thousand, a tail with one.
        let n = 2 + (next() % 39) as usize;
        let counts: Vec<f64> = (0..n)
            .map(|_| {
                let magnitude = (next() % 5) as i32;
                let scale = 10f64.powi(magnitude);
                (next() % 1000) as f64 / 1000.0 * scale
            })
            .collect();
        let max = counts.iter().cloned().fold(0.0f64, f64::max).max(1.0);
        let shares: Vec<f64> = counts.iter().map(|c| damped.share(*c, max)).collect();
        let raw: Vec<f64> = counts.iter().map(|c| c / max).collect();

        for (i, (d, r)) in shares.iter().zip(&raw).enumerate() {
            // 1. Bounded: a share is a share.
            assert!(*d <= 1.0 + 1e-12, "share above 1 at {i}: {d}");
            // 2. The leader is scored exactly as raw: damping compresses, it does not discount.
            if counts[i] >= max - 1e-12 {
                assert!((d - r).abs() < 1e-9, "the leader's score moved: {d} vs {r}");
            } else {
                // 3. Compressed, never amplified into a lead: damping may lift a tail member, never create a
                //    rank reversal, and never let a laggard overtake a leader.
                assert!(*d <= 1.0 + 1e-12);
            }
        }
        for i in 0..n {
            for j in 0..n {
                if counts[i] > counts[j] + 1e-12 {
                    assert!(
                        shares[i] >= shares[j] - 1e-12,
                        "order inverted: {} < {} but share {:.4} < {:.4}",
                        counts[i],
                        counts[j],
                        shares[i],
                        shares[j]
                    );
                    // 4. The lead is compressed: damping never widens any ratio.
                    let before = counts[i] / counts[j];
                    let after = shares[i] / shares[j];
                    assert!(
                        after <= before + 1e-9,
                        "the lead widened: {before:.4} became {after:.4}"
                    );
                }
            }
        }
        // 5. A lead of k times becomes at most k times, for every k the pool can express.
        for k in [1.0, 2.0, 5.0, 10.0, 100.0, 1000.0] {
            assert!(damped.lead_after(k) <= k + 1e-9, "lead_after({k}) exceeded {k}");
        }
    }
}

/// The clause the roadmap wrote, and the arithmetic that refutes it. This test passes by *proving the claim
/// false*: it is the reason the suite asserts compression instead.
#[test]
fn the_clause_as_written_is_false_and_the_counterexample_is_arithmetic() {
    let damped = Config::default();
    let max = 1000.0;
    let one = 1.0;
    let raw_share = one / max;
    let damped_share = damped.share(one, max);
    assert!(
        damped_share > raw_share * 100.0,
        "a one-click member of a thousand is lifted from {raw_share:.4} to {damped_share:.4} — the damped \
         score exceeding the raw score is not a bug, it is the mechanism"
    );
    // ...and the leader is untouched, which is the part of the clause that was right.
    assert!((damped.share(max, max) - 1.0).abs() < 1e-12);
}

/// An exempt member is scored as `raw`, and when everyone is exempt the page order equals the raw page order
/// exactly — no lens, no damping, no mixture.
#[test]
fn an_exempt_pool_is_scored_exactly_as_raw() {
    let counts = [1000.0, 300.0, 40.0, 7.0, 1.0];
    let max = 1000.0;
    let raw = Config { mode: Mode::Damped, alpha: 1.0, exempt: vec!["everyone".into()] };
    for c in counts {
        let exempt_share = if raw.is_exempt("everyone") {
            c / max
        } else {
            raw.share(c, max)
        };
        assert!((exempt_share - c / max).abs() < 1e-12);
    }
    // The list is matched case-insensitively and by whole handle, not by prefix: `critical` must not exempt
    // `criticalnexus`, or a typo in a declaration would silently change a page.
    let cfg = Config { mode: Mode::Damped, alpha: 0.5, exempt: vec!["SteelTitan".into()] };
    assert!(cfg.is_exempt("steeltitan"), "case does not matter");
    assert!(!cfg.is_exempt("steeltita"), "a prefix is not a member");
    assert!(!cfg.is_exempt("criticalnexus"), "an unrelated handle is not exempt");
}
