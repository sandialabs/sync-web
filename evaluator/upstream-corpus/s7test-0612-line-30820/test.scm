;; Imported from upstream s7test.scm line 30820.
;; Original form:
;; (test (catch #t (lambda () (let ((lti (make-iterator (inlet 'a 1 'b 2)))) (set! (lti) 32))) (lambda (typ info) (apply format #f info)))
;;       "lti (an iterator) does not have a setter: (set! (lti) 32)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((lti (make-iterator (inlet 'a 1 'b 2)))) (set! (lti) 32))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "lti (an iterator) does not have a setter: (set! (lti) 32)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30820 actual expected ok?))
