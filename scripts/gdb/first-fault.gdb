set pagination off
set confirm off
set architecture i386:x86-64
file target/x86_64-unknown-none/release/cognacos-kernel
target remote 127.0.0.1:12345
hbreak cognacos_exception_dispatch
condition 1 $rdi == 14
commands 1
  silent
  printf "\n===== GDB FIRST PAGE-FAULT EXCEPTION (ANY CPL) =====\n"
  printf "vector=%#lx error=%#lx frame=%#lx\n", $rdi, $rsi, $rdx
  printf "RIP=%#lx CS=%#lx RFLAGS=%#lx RSP=%#lx SS=%#lx\n", \
    *(unsigned long long *)($rdx + 136), *(unsigned long long *)($rdx + 144), \
    *(unsigned long long *)($rdx + 152), *(unsigned long long *)($rdx + 160), \
    *(unsigned long long *)($rdx + 168)
  info registers cr2 cr3
  x/22gx $rdx
  set $fault_rip = *(unsigned long long *)($rdx + 136)
  printf "===== FAULTING INSTRUCTIONS (kernel or user; runtime-generated stub or PE image) =====\n"
  x/24i $fault_rip-24
  info symbol $fault_rip
  info line *$fault_rip
  printf "===== END GDB FIRST RING-3 EXCEPTION =====\n"
  continue
end
continue
