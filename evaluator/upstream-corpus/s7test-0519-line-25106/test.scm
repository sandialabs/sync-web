;; Imported from upstream s7test.scm line 25106.
;; Original form:
;; (test (object->string (let ((iter (make-iterator #u(12)))) (iter) (iter) iter) :readable) "(make-iterator #u())")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((iter (make-iterator #u(12)))) (iter) (iter) iter) :readable))))
       (expected (upstream-safe (lambda () "(make-iterator #u())")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25106 actual expected ok?))
