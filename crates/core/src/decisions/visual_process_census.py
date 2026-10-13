"""Darwin compiler census: native PID/PPID/short name, never argv or env.

PROC_PIDT_SHORTBSDINFO (13), proc_bsdshortinfo (64 bytes), and MAXCOMLEN
(16) are the public Darwin libproc/sys/proc_info.h ABI. Output is complete
before printing, bounded to 65,536 rows, and uses only compiler labels.
"""
import ctypes
import os
import sys
import time

MAX_PIDS = 65536
BUILDERS = {b'cargo', b'cargo-clippy', b'rustc'}


class Refusal(ValueError):
    pass


class ShortInfo(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint32) for name in ('pid', 'ppid', 'pgid', 'status')]
    _fields_ += [('comm', ctypes.c_char * 16)]
    _fields_ += [(name, ctypes.c_uint32) for name in
                ('flags', 'uid', 'gid', 'ruid', 'rgid', 'svuid', 'svgid', 'reserved')]


class Native:
    def __init__(self):
        if sys.platform != 'darwin' or ctypes.sizeof(ShortInfo) != 64:
            raise Refusal('native process ABI is unavailable')
        self.api = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
        self.api.proc_listallpids.argtypes = [ctypes.c_void_p, ctypes.c_int]
        self.api.proc_listallpids.restype = ctypes.c_int
        self.api.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                        ctypes.c_void_p, ctypes.c_int]
        self.api.proc_pidinfo.restype = ctypes.c_int

    def pids(self):
        count = self.api.proc_listallpids(None, 0)
        if not 0 < count <= MAX_PIDS - 1024:
            raise Refusal('native process list is unavailable or exceeds its bound')
        capacity = count + 1024
        buffer = (ctypes.c_int * capacity)()
        returned = self.api.proc_listallpids(buffer, ctypes.sizeof(buffer))
        if not 0 < returned < capacity:
            raise Refusal('native process list is incomplete')
        pids = list(buffer[:returned])
        if len(set(pids)) != len(pids) or any(pid < 0 for pid in pids):
            raise Refusal('native process list is invalid')
        # PID zero is Darwin's kernel task, never a user compiler process.
        return set(pids) - {0}

    def row(self, pid):
        info = ShortInfo()
        returned = self.api.proc_pidinfo(pid, 13, 0, ctypes.byref(info), ctypes.sizeof(info))
        if returned != ctypes.sizeof(info) or info.pid != pid:
            raise Refusal('native process row is unavailable')
        name = bytes(info.comm)
        if not name:
            raise Refusal('native process name is unavailable')
        label = name.decode('ascii') if name in BUILDERS else 'other'
        return info.ppid, label


def checked_pids(native):
    pids = native.pids()
    if (not pids or len(pids) > MAX_PIDS or os.getpid() not in pids
            or any(type(pid) is not int or pid <= 0 for pid in pids)):
        raise Refusal('native process census is empty or incomplete')
    return pids


def census(native, clock=time.monotonic):
    deadline = clock() + 2
    # Retry only enumeration churn. An unreadable process that remains in the
    # final list prevents success; partial metadata can never become zero.
    for _ in range(3):
        rows = {}
        unknown = set()
        for pid in checked_pids(native):
            if clock() >= deadline:
                raise Refusal('native process census exceeded its time bound')
            try:
                parent, label = native.row(pid)
            except Refusal:
                unknown.add(pid)
                continue
            if type(parent) is not int or parent < 0 or label not in ('cargo', 'cargo-clippy', 'rustc', 'other'):
                raise Refusal('native process row is invalid')
            rows[pid] = (parent, label)
        final = checked_pids(native)
        if unknown & final:
            raise Refusal('native process census has unknown rows')
        if clock() >= deadline:
            raise Refusal('native process census exceeded its time bound')
        if final <= rows.keys():
            return ''.join(f'{pid} {rows[pid][0]} {rows[pid][1]}\n' for pid in sorted(final))
    raise Refusal('native process census has unknown rows')


if __name__ == '__main__':
    try:
        sys.stdout.write(census(Native()))
    except (OSError, Refusal) as error:
        sys.exit('native process census refused: ' + str(error))
