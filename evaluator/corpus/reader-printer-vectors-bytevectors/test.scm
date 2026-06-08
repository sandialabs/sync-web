(list
  (list 'vector #(1 two "three"))
  (list 'vector-ref (#(10 20 30) 1))
  (list 'byte-vector #u(0 1 2 255))
  (list 'byte-vector? (byte-vector? #u(1 2 3))))
