;; Imported from upstream s7test.scm line 25095.
;; Original form:
;; (test (object->string (define (ex1 a b) (+ a  b)) :readable) "(lambda (a b) (+ a b))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (define (ex1 a b) (+ a  b)) :readable))))
       (expected (upstream-safe (lambda () "(lambda (a b) (+ a b))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25095 actual expected ok?))
