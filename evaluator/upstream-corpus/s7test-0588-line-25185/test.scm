;; Imported from upstream s7test.scm line 25185.
;; Original form:
;; (test (object->string (inlet 'a (let ((p (open-input-string "1"))) (close-input-port p) p)) :readable) "(inlet :a (call-with-input-string \"\" (lambda (p) p)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (let ((p (open-input-string "1"))) (close-input-port p) p)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (call-with-input-string \"\" (lambda (p) p)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25185 actual expected ok?))
