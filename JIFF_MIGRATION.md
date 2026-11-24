# Jiff and timezone data

Date/time handling uses native Jiff types. Named timezone conversions explicitly
use Jiff's bundled IANA database, independent of the host zoneinfo. CI checks the
resolved `jiff-tzdb` crate's bundled release against IANA. After an IANA update,
use the corresponding `jiff-tzdb` release and rebuild consumers to load the new
data. The older generated TZif files under `src/tzdata` are no longer read at
runtime.

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
