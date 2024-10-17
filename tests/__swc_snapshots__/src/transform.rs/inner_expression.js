let x = doIt(template(`Hello`, { eval() { return eval(arguments[0]) }}))
