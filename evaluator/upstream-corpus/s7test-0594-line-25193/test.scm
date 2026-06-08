;; Imported from upstream s7test.scm line 25193.
;; Original form:
;; (test (object->string
;;        (inlet 'a (let ((p (open-output-string))) (display 32 p) p)) :readable)
;;       "(inlet :a (let ((p (open-output-string))) (display \"32\" p) p))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string
       (inlet 'a (let ((p (open-output-string))) (display 32 p) p)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((p (open-output-string))) (display \"32\" p) p))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25193 actual expected ok?))
