;; Imported from upstream s7test.scm line 25189.
;; Original form:
;; (test (object->string
;;        (inlet 'a (let ((p (open-output-string))) (close-output-port p) p)) :readable)
;;       "(inlet :a (let ((p (open-output-string))) (close-output-port p) p))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string
       (inlet 'a (let ((p (open-output-string))) (close-output-port p) p)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((p (open-output-string))) (close-output-port p) p))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25189 actual expected ok?))
