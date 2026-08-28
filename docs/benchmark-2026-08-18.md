# Benchmark record · 2026-08-18

Run: `20260818-035635-50c5fc9a`

- One model for both sides: `hy3-free`
- Ten coding repair tasks
- One run per agent and task
- Score weights: completion 50%, token efficiency 30%, speed 10%, flexibility 10%

| Metric | KnightFrame | Baseline |
| --- | ---: | ---: |
| Completed | 9 / 10 | 8 / 10 |
| Total tokens | 426,941 | 1,610,622 |
| Input tokens | 329,991 | 1,507,598 |
| Output tokens | 96,950 | 103,024 |
| Requests | 65 | 101 |
| Elapsed | 1,514,923 ms | 1,879,064 ms |
| Reference cost | $0.03483 | $0.06920 |
| Weighted score | 85.17 | 68.70 |

Reference cost is a normalized estimate, not the provider bill. A single run is useful for diagnosis, not for a general ranking.
