;; Imported from upstream s7test.scm line 25174.
;; Original form:
;; (test (object->string (inlet 'a (let ((b 1)) (lambda () b))) :readable) "(inlet :a (let ((b 1)) (lambda () b)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (let ((b 1)) (lambda () b))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((b 1)) (lambda () b)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25174 actual expected ok?))
