;; Imported from upstream s7test.scm line 25117.
;; Original form:
;; (test (object->string (inlet 'a #\null) :readable) "(inlet :a #\\null)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a #\null) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a #\\null)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25117 actual expected ok?))
