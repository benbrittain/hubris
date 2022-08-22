target extended-remote :2331

set print asm-demangle on

monitor reset
load

monitor SWO EnableTarget 0 0 1 0

