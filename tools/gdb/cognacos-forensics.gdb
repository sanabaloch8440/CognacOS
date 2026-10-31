# CognacOS x86-64 kernel forensic helpers.
# Start QEMU with: -S -gdb tcp::1234
# Then: gdb -q -x tools/gdb/cognacos-forensics.gdb build/kernel.debug.elf
# If symbols are separate, pass the ELF with DWARF as the final GDB argument.
#
# This file deliberately does not auto-continue or change kernel state.

set pagination off
set confirm off
set print pretty on
set print demangle on
set disassembly-flavor intel
set disassemble-next-line on
set history save on
set history filename .gdb_history
set backtrace past-main on
set unwindonsignal on
set remotetimeout 20
set trust-readonly-sections on

define cognacos-regs
  printf "\n===== General registers =====\n"
  info registers
  printf "\n===== Segment/control registers (when target supports them) =====\n"
  # info registers reports the register set exposed by this target; do not
  # assume every QEMU/GDB target exposes control or segment registers by name.
end

define cognacos-stack
  printf "\n===== Stack words at RSP =====\n"
  x/32gx $rsp
  printf "\n===== Stack disassembly (best effort) =====\n"
  disassemble /r $rip-32, $rip+64
end

define cognacos-pc
  printf "\n===== Current PC =====\n"
  info symbol $rip
  info line *$rip
  x/16i $rip
  disassemble /r $rip-32, $rip+64
end

define cognacos-forensics
  set logging file cognacos-gdb-forensics.txt
  set logging overwrite on
  set logging enabled on
  printf "\n######## COGNACOS FORENSIC SNAPSHOT ########\n"
  show architecture
  show endian
  cognacos-regs
  cognacos-pc
  cognacos-stack
  printf "\n===== Backtrace =====\n"
  bt full
  printf "\n===== Frame and arguments =====\n"
  info frame
  info args
  info locals
  printf "\n===== Loaded sections / symbol mapping =====\n"
  maintenance info sections
  info files
  printf "\n===== Breakpoints / watchpoints =====\n"
  info breakpoints
  printf "\n######## END FORENSIC SNAPSHOT ########\n"
  set logging enabled off
  printf "Saved cognacos-gdb-forensics.txt\n"
end

define cognacos-exception-breaks
  # Set only symbols that exist in the currently loaded image.
  break cognacos_isr13
  break cognacos_isr14
  break cognacos_page_fault_handler
  break cognacos_general_protection_fault_handler
  info breakpoints
end

define cognacos-watch-return
  # Call at a known-good point before a suspect return path.
  # Hardware watchpoints are limited; use one at a time and select an
  # aligned address in writable RAM (for example the saved RSP slot).
  if $argc != 1
    printf "Usage: cognacos-watch-return ADDRESS\n"
  else
    watch *(unsigned long long*)$arg0
  end
end

document cognacos-forensics
Capture a reproducible register, stack, instruction, DWARF, and mapping snapshot
to cognacos-gdb-forensics.txt. Does not continue the target.
end

document cognacos-exception-breaks
Try to set breakpoints on common x86-64 exception handlers. If symbol names
differ in this build, use 'info functions exception' and set them manually.
end

printf "CognacOS GDB forensic helpers loaded. Commands: cognacos-regs, cognacos-pc, cognacos-stack, cognacos-forensics, cognacos-exception-breaks.\n"
