gdb ./bad --command=show_cycle.gdb -ex "show_cycle $1" > badout
gdb ./good --command=show_cycle.gdb -ex "show_cycle $1" > goodout
delta goodout badout --max-line-length 100000
