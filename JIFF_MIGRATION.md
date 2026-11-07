# Jiff and timezone data

Date/time handling now uses native Jiff types. Named timezone conversions use
an explicit package-local TZif bundle generated from IANA 2026f, including in
crates.io packages. This avoids depending on the host zoneinfo version or waiting
for the published jiff-tzdb crate (0.1.9 currently bundles 2026e) to catch up.

`python3 tools/update-tzdata.py 2026f` regenerates the selected zones. It builds
zic from the matching IANA code release, compiles Unix/POSIX timestamps without
leap-second corrections, and records both source archive SHA-256 hashes.
IANA tzdata is public domain; see https://data.iana.org/time-zones/tzdb/LICENSE.

Daily CI checks the latest IANA release and opens update PRs. Stale data fails
the freshness check. GitHub Actions needs permission to create PRs. Changes
require merge, a new crate release, and consumer rebuild/deployment; a running
binary cannot update its packaged data automatically.

Chrono may remain in transitive GTFS dependencies, but is no longer a direct
dependency or used in application source. Values belonging to GTFS structures
are converted at their API boundaries. Existing service-day and DST ambiguity
policies are retained. Jiff's supported year range is -9999 through 9999.

## 0.10 API changes

`origin_departure` now returns `jiff::Zoned`. Use `.timestamp().as_second()`
for Unix seconds, `.date()` for a civil service date, `.with_time_zone(...)`
for conversion, and `.strftime(...)` for formatting. ASM lookup keys and
`stop_times` date parameters now use `jiff::civil::Date` instead of Chrono dates.
The minor version is bumped because these exposed types change.

## Optional HTTP session configuration

The obsolete hardcoded cookie in the stop-times helper has been removed.
Deployments that need a session cookie can supply `AMTRAK_COOKIE`; requests
without it omit the Cookie header. No credential value is stored in source.
