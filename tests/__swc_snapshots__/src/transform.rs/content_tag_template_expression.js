let x = template(`Hello`, { eval() { return eval(arguments[0]); }})
