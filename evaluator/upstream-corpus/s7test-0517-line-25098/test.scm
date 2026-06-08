;; Imported from upstream s7test.scm line 25098.
;; Original form:
;; (test (object->string (define* (ex1 a (b 0)) (+ a  b)) :readable) "(lambda* (a (b 0)) (+ a b))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (define* (ex1 a (b 0)) (+ a  b)) :readable))))
       (expected (upstream-safe (lambda () "(lambda* (a (b 0)) (+ a b))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25098 actual expected ok?))
