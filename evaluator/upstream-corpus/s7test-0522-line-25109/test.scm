;; Imported from upstream s7test.scm line 25109.
;; Original form:
;; (test (object->string (let ((iter (make-iterator #i(1)))) (iter) (iter) iter) :readable) "(make-iterator #i())")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((iter (make-iterator #i(1)))) (iter) (iter) iter) :readable))))
       (expected (upstream-safe (lambda () "(make-iterator #i())")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25109 actual expected ok?))
