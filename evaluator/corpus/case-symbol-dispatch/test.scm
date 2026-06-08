(define (dispatch msg . args)
  (case msg
    ((get) (car args))
    ((sum) (apply + args))
    ((quote) 'quoted)
    (else (list 'unknown msg args))))

(list
  (dispatch 'get 'value)
  (dispatch 'sum 1 2 3)
  (dispatch 'missing 'a 'b))
