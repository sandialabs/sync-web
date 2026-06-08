;; Imported from upstream s7test.scm line 25150.
;; Original form:
;; (test (or (equal? (object->string (inlet 'a abs) :readable) "(inlet :a abs)") (equal? (object->string (inlet 'a abs) :readable) "(inlet :a #_abs)")) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (equal? (object->string (inlet 'a abs) :readable) "(inlet :a abs)") (equal? (object->string (inlet 'a abs) :readable) "(inlet :a #_abs)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25150 actual expected ok?))
