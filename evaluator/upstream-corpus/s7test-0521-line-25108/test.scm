;; Imported from upstream s7test.scm line 25108.
;; Original form:
;; (test (object->string (let ((iter (make-iterator #r(1)))) (iter) (iter) iter) :readable) "(make-iterator #r())")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((iter (make-iterator #r(1)))) (iter) (iter) iter) :readable))))
       (expected (upstream-safe (lambda () "(make-iterator #r())")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25108 actual expected ok?))
