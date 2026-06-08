(define (normalize-query query)
  (let ((function (cadr (assoc 'function query)))
        (arguments (assoc 'arguments query)))
    (list function (if arguments (cadr arguments) '()))))

(map normalize-query
     '(((function get) (arguments ((path (*state* alice doc)))))
       ((function set!) (arguments ((path (*state* alice doc)) (value #u(1 2 3)))))))
