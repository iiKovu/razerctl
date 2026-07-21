# `razer-cfg255` DKMS compatibility module

The Xbox dongle chooses whether to enable its vendor control channel during its
first USB configuration-descriptor read after power-on. Windows asks for 255
bytes; Linux normally asks for a 9-byte header first. The module changes that
one request length for the known BlackShark V3 device IDs.

This is separate from the userspace daemon. The daemon supplies the battery,
EQ, sidetone, THX, ANC, and power controls after enumeration; the DKMS module
makes sure the dongle exposes the working control state during Linux
enumeration.

The canonical source matching version 1.0 is in `razer-cfg255-1.0/`. Installing
or removing it requires root and is intentionally not performed by the portable
launcher.
