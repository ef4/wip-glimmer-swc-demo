let x = template(`He\${ll}o`, { eval() { return eval(arguments[0]) }})
