set pagination off
set confirm off
set print pretty on
set print frame-arguments all
set disassemble-next-line on
set backtrace limit 80
set architecture i386:x86-64
set substitute-path /home/runner/work/CognacOS/CognacOS .
file target/x86_64-unknown-none/debug/cognacos-kernel
target remote 127.0.0.1:12345
source tools/gdb/cognacos-forensics.gdb
cognacos-forensics
echo \n===== LIVE CPU / SOURCE LOCATION =====\n
info registers
info variables XHCI_PCI_DEVICE
info variables PCI_BUS_FDO
info variables VT_D_REQUESTER_OWNER
info variables XHCI_INTERRUPT_VECTOR
info variables XHCI_DRIVER_BINDING_STARTED
echo \n===== PCI / VT-d / xHCI static state =====\n
x/gx &cognacos_kernel::PCI_BUS_FDO
x/bx &cognacos_kernel::PCI_DRIVER_BINDING_STARTED
x/16gx &cognacos_kernel::ntoskrnl::pnp::XHCI_PCI_DEVICE
x/wx &cognacos_kernel::VT_D_REQUESTER_OWNER
x/wx &cognacos_kernel::ntoskrnl::pnp::XHCI_INTERRUPT_VECTOR
x/bx &cognacos_kernel::XHCI_DRIVER_BINDING_STARTED
echo \n===== PCI.sys inventory state (PE base 0xffff8000e0000000) =====\n
python
import gdb
for expression in ("x/4gx 0xffff8000e0006070", "x/gx 0xffff8000e0064318"):
    try:
        gdb.execute(expression)
    except gdb.error as error:
        print("Optional PCI image memory inspection unavailable: " + str(error))
end
info threads
thread apply all info registers rip rsp
thread apply all info line *$rip
info line *$rip
list *$rip
info symbol $rip
x/32i $rip-32
x/32gx $rsp
echo \n===== KERNEL BACKTRACE WITH SOURCE LINES =====\n
thread apply all bt full
echo \n===== HAL SYMBOL COMPANION =====\n
add-symbol-file target/driver-hal/x86_64-unknown-none/release/hal
info symbol $rip
info line *$rip
echo \n===== NTDLL SYMBOL/EXPORT CHECK =====\n
info sharedlibrary
maintenance info sections
detach
quit
