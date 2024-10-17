let x = template(`He\`llo`, { eval() { return eval(arguments[0]) }})
