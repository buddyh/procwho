/* Compile-only ABI check; run for arm64 and x86_64 with the macOS SDK.
 * xcrun clang -arch x86_64 -mmacosx-version-min=13.0 -fsyntax-only tests/darwin_abi.c
 * The fixed-width libproc structures have the same LP64 layout on both architectures.
 */
#include <stddef.h>
#include <sys/proc_info.h>
#include <sys/resource.h>

_Static_assert(sizeof(struct proc_bsdinfo) == 136, "proc_bsdinfo layout changed");
_Static_assert(sizeof(struct proc_bsdshortinfo) == 64, "proc_bsdshortinfo layout changed");
_Static_assert(sizeof(struct vnode_info) == 152, "vnode_info layout changed");
_Static_assert(offsetof(struct vnode_info_path, vip_path) == 152, "cwd path offset changed");
_Static_assert(sizeof(struct proc_vnodepathinfo) == 2352, "cwd structure size changed");
_Static_assert(sizeof(struct rusage_info_v4) == 296, "rusage v4 layout changed");
_Static_assert(offsetof(struct rusage_info_v4, ri_resident_size) == 64, "resident offset changed");
_Static_assert(offsetof(struct rusage_info_v4, ri_phys_footprint) == 72, "footprint offset changed");
_Static_assert(offsetof(struct rusage_info_v4, ri_lifetime_max_phys_footprint) == 240, "peak footprint offset changed");
