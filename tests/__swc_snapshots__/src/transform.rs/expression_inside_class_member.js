class X {
  thing = template(`Hello`, { eval() { return eval(arguments[0]) }},);
}