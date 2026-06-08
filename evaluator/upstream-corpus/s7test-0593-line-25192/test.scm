;; Imported from upstream s7test.scm line 25192.
;; Original form:
;; (test (object->string (inlet 'a (open-output-string)) :readable) "(inlet :a (let ((p (open-output-string))) p))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (open-output-string)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((p (open-output-string))) p))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25192 actual expected ok?))
