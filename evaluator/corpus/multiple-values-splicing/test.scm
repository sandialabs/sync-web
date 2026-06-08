(list
  (list '+ (+ (values 1 2 3) 4))
  (list 'apply ((lambda (a b c) (list a b c)) (values 'x 'y 'z)))
  (list 'nested (list 'a (values 'b 'c) 'd)))
