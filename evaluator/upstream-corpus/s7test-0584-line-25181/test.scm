;; Imported from upstream s7test.scm line 25181.
;; Original form:
;; (test (object->string (inlet 'a (let ((p (open-input-string "123456"))) (read-char p) p)) :readable) "(inlet :a (open-input-string \"23456\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (let ((p (open-input-string "123456"))) (read-char p) p)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (open-input-string \"23456\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25181 actual expected ok?))
