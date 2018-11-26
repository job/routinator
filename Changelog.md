# Change Log

## Next Version

Breaking Changes

* Add trust anchor information to the CSV, JSON, and RPSL output. [21]
* The --man option now spawns the man command instead of just outputting
  the man page.
* The cache-dir option now has the short option `-C` to make room for `-c`
  being the config file.

New

* Configuration can now be read from a config file via the -c option.

Bug Fixes

* The default output format was accidentally changed to `none`. It is
  `csv` again.
* Thanks to copy-and-paste, the value of the history option was accidentally
  read from the refresh command line argument.

Dependencies

[21]: https://github.com/NLnetLabs/routinator/pull/21


## 0.1.2 ‘And I Cry If I Want To’

Bug Fixes

* [Panic in iterating over the withdrawals in an RTR set][17].
* When comparing serial numbers for RTR Serial Query, looked at the oldest
  known serial not the newest, always returning an empty change set.

[17]: https://github.com/NLnetLabs/routinator/issues/17


## 0.1.1 ‘Five-second Rule’

Bug Fixes

* [Wrong End Of Data PDU in RPKI-RTR version 0.][15]

[15]: https://github.com/NLnetLabs/routinator/issues/15


## 0.1.0 ‘Godspeed!’

Initial public release.

