target extended-remote :2331

set print asm-demangle on

monitor reset
load
# monitor semihosting enable
monitor SWO EnableTarget 0 0 1 0

# break task_blinky::main
