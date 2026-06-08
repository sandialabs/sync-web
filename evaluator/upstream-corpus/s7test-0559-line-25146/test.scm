;; Imported from upstream s7test.scm line 25146.
;; Original form:
;; (test (object->string (inlet 'a (vector "hi" #\a 'b)) :readable) "(inlet :a (vector \"hi\" #\\a 'b))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (vector "hi" #\a 'b)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (vector \"hi\" #\\a 'b))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25146 actual expected ok?))
