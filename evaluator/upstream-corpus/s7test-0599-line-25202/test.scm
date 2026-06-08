;; Imported from upstream s7test.scm line 25202.
;; Original form:
;; (test (object->string
;;        (inlet 'a (let ((iter (make-iterator (float-vector 1 2 3)))) (iter) iter)) :readable)
;;       "(inlet :a (let ((iter (make-iterator #r(1.0 2.0 3.0)))) (iter) iter))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string
       (inlet 'a (let ((iter (make-iterator (float-vector 1 2 3)))) (iter) iter)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((iter (make-iterator #r(1.0 2.0 3.0)))) (iter) iter))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25202 actual expected ok?))
