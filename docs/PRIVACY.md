# Privacy

ForgePulse operates locally and requires no account, subscription, cloud model, or
external service. Telemetry is off and no telemetry endpoint exists in this phase.

The collector records performance counters and process metadata needed for local
diagnosis. It never records keystrokes, clipboard data, typed text, passwords,
screenshots, microphone data, packet contents, or private document contents.
Executable paths stay local. Export anonymization removes usernames, machine names,
IP addresses, network names, serial numbers, and personal path prefixes before a
report is written.

Users can configure retention and delete the local database while the service is
stopped. Future features that widen collection scope must add an explicit setting,
data description, and retention rule before implementation.

