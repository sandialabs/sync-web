;; Imported from upstream s7test.scm line 25182.
;; Original form:
;; (test (object->string (inlet 'a (let ((p (open-input-string "1"))) (read-char p) p)) :readable) "(inlet :a (open-input-string \"\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (let ((p (open-input-string "1"))) (read-char p) p)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (open-input-string \"\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25182 actual expected ok?))
