(macro (config standard-module chain tree ledger federation authorization . classes)
  (if (not (equal? (sync-digest *sync-state*)
                   #u(0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
                      0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0)))
      (error 'upgrade-error "This Interface supports fresh installation only"))
  (set! chain (eval chain))
  (set! tree (eval tree))
  (set! ledger (eval ledger))
  (set! federation (eval federation))
  (set! authorization (eval authorization))
  (define (cfg key) (cadr (assoc key config)))
  (for-each (lambda (key)
              (if (not (assoc key config)) (error 'argument-error "Missing config")))
            '(root-secret interface-secret root interface name))
  (if (equal? (sync-hash (expression->byte-vector (cfg 'root-secret)))
              (sync-hash (expression->byte-vector (cfg 'interface-secret))))
      (error 'argument-error "Root and Interface credentials must differ"))
  (set! config (append config '((clear? #t) (admins ()) (window #f)
                                (bridge-accept auto))))
  (if (not (cfg 'clear?))
      (error 'upgrade-error "This Interface supports fresh installation only"))

  (define standard-source (eval standard-module))
  (define standard-module* (eval standard-source))
  (define standard-class (standard-module* 'class))
  (define (call body) (let ((result (sync-call `(*call* ,(cfg 'root-secret) ,body) #t)))
			(if (and (pair? result) (eq? (car result) 'error)) (apply error (cdr result)) result)))
  (define (set-query body) (sync-call `(*set-query* ,(cfg 'root-secret) ,body) #t))
  (define (set-step body) (sync-call `(*set-step* ,(cfg 'root-secret) ,body) #t))

  (let ((result (sync-call `(,(cfg 'root) ,(cfg 'root-secret) fresh) #t)))
    (if (and (pair? result) (eq? (car result) 'error))
        (apply error (cdr result))))
  (call `(lambda (root) (let* ((standard-module (eval ',standard-source))
			       (standard-node (standard-module 'make))
			       (standard-class (standard-module 'class))
			       (identity-nonce ,(random-byte-vector 32))
			       (identity-id (sync-hash (expression->byte-vector
							(list 'sync-web/journal-id/v1 identity-nonce))))
			       (identity `((id ,identity-id) (nonce ,identity-nonce)))
			       (journal-keys (crypto-generate (expression->byte-vector
							       (list 'sync-web/journal-signing-key/v1 identity-id
								     (sync-hash (expression->byte-vector ,(cfg 'root-secret)))))))
                               (interface-keys
                                (crypto-generate
                                 (expression->byte-vector
                                  (list 'sync-web/interface-signing-key/v1 identity-id
                                        (sync-hash
                                         (expression->byte-vector ,(cfg 'interface-secret)))))))
			       (federation-node
                                (standard-module 'init ',federation
                                 (list standard-node
                                       `((endpoint ,,(cfg 'interface))
                                         (bridge-accept ,',(cfg 'bridge-accept))
                                         (public-key ,(car interface-keys))
                                         (peers ()) (retired ())))))
                               (federation-live
                                (standard-module 'local ',federation federation-node))
                               (interface-public-key (car interface-keys))
                               (ledger-config
                                `((public
                                   ((window ,,(cfg 'window)) (identity ,identity)
                                    (public-key ,(car journal-keys))
                                    (journal ((public-key ,(car journal-keys))
                                              (latest-rotation-index -1)))
                                    (interface ((public-key ,interface-public-key)
                                                (endpoint ,,(cfg 'interface))))
                                    (bridge-accept ,',(cfg 'bridge-accept))
                                    (name ,',(cfg 'name))))
                                  (private
                                   ((bridge-preapproval ())
                                    (journal ((rotation-indexes ())
                                              (pending-rotation ())))))))
                               (ledger-node
                                (standard-module 'init ',ledger
                                 (list standard-node ledger-config ',tree ',chain)))
                               (authorization-node
                                (standard-module 'init ',authorization '())))
			  ((root 'set!) '(root class standard-module) ',standard-source)
			((root 'set!) '(root class standard) standard-class)
			  ((root 'set!) '(root class chain) ',chain)
			  ((root 'set!) '(root class tree) ',tree)
			  ((root 'set!) '(root class ledger) ',ledger)
			  ((root 'set!) '(root class federation) ',federation)
			  ((root 'set!) '(root class authorization) ',authorization)
			  ((root 'set!) '(root object standard) standard-node)
			  ((root 'set!) '(root object ledger) ledger-node)
			  ((root 'set!) '(root object federation) (federation-live))
			  ((root 'set!) '(root object authorization) authorization-node)
                          ((root 'set!) '(interface credential) ,(cfg 'interface-secret))
                          ((root 'set!) '(interface root-verifier)
                           (sync-hash (expression->byte-vector ,(cfg 'root-secret))))
			  ((root 'set!) '(interface secret)
			   (sync-hash (expression->byte-vector ,(cfg 'interface-secret))))
			  ((root 'set!) '(interface admins) ',(cfg 'admins))
			  ((root 'set!) '(interface endpoint) ,(cfg 'interface))
			  ((root 'set!) '(interface name) ',(cfg 'name))
			  #t)))

  (let loop ((rest classes)) (if (pair? rest) (begin (call `(lambda (root)
							      ((root 'set!) '(root object ,(caar rest)) ,(cadar rest))))
						     (loop (cdr rest)))))

  (call `(lambda (root) ((root 'set!) '(root handler secret) (expression->byte-vector
							      '(lambda (root old-secret new-secret)
                                                                   (if (equal? (sync-hash (expression->byte-vector new-secret))
                                                                               ((root 'get) '(interface secret)))
                                                                       (error 'argument-error "Root and Interface credentials must differ"))
								 (let* ((module (eval ((root 'get) '(root class standard-module))))
									(std-node ((root 'get) '(root object standard)))
									(standard (module 'local ((root 'get) '(root class standard)) std-node))
									(ledger (module 'local ((root 'get) '(root class ledger))
										((root 'get) '(root object ledger))))
									(identity ((ledger 'config) '(public identity)))
									(id (cadr (assoc 'id identity)))
									(old (crypto-generate (expression->byte-vector
											       (list 'sync-web/journal-signing-key/v1 id (sync-hash
																	  (expression->byte-vector old-secret))))))
									(new (crypto-generate (expression->byte-vector
											       (list 'sync-web/journal-signing-key/v1 id (sync-hash
																	  (expression->byte-vector new-secret))))))
									(indexes ((ledger 'config) '(journal-rotation-indexes)))
									(previous (if (null? indexes) -1 (car (reverse indexes))))
									(index ((ledger 'size))))
								   (if (not (equal? (car old) (car new))) (begin ((ledger 'step!)
														  `((unix-time ,(system-time-unix))
														    (public-key ,(car new)) (secret-key ,(cdr new))
														    (rotation ((previous-key ,(car old)) (public-key ,(car new)) (signature
																						  ,(crypto-sign (cdr old) (expression->byte-vector
																									   (list 'sync-web/journal-key-rotation/v1 id index
																										 previous (car old) (car new)))))))))
														 ((root 'set!) '(root object ledger) (ledger))))
                                                                   ((root 'set!) '(interface root-verifier)
                                                                    (sync-hash (expression->byte-vector new-secret)))
								   #t))))))

  (define query-once '(lambda (query) (let* ((function (cadr (assoc 'function query)))
					     (arguments (if (assoc 'arguments query) (cadr (assoc 'arguments query)) '()))
					     (authentication (and (assoc 'authentication query)
								  (cadr (assoc 'authentication query))))
					     (invocation (and (assoc 'invocation query) (cadr (assoc 'invocation query))))
					     (prepared (and (assoc 'prepared query) (cadr (assoc 'prepared query))))
					     (module (eval ((root 'get) '(root class standard-module))))
                                             (std-node ((root 'get) '(root object standard)))
                                             (standard (module 'local ((root 'get) '(root class standard)) std-node))
                                             (ledger (module 'local ((root 'get) '(root class ledger))
                                                             ((root 'get) '(root object ledger))))
                                             (federation (module 'local ((root 'get) '(root class federation))
                                                                 ((root 'get) '(root object federation))))
                                             (authorization (module 'local ((root 'get) '(root class authorization))
                                                                    ((root 'get) '(root object authorization))))
                                             (context '()) (auth-head #f) (ancestor? #f)
					     (delegated? #f))

					(define (arg name) (let ((entry (assoc name arguments))) (and entry (cadr entry))))
                                        (define (transient-key domain secret)
                                          (let* ((identity ((ledger 'config) '(public identity)))
                                                 (id (cadr (assoc 'id identity))))
                                            (crypto-generate
                                             (expression->byte-vector
                                              (list domain id
                                                    (sync-hash
                                                     (expression->byte-vector secret)))))))
                                        (define (request-secret)
                                          (or (arg-from authentication 'credentials)
                                              (arg-from invocation 'credentials)
                                              (error 'authentication-error
                                                     "Signing requires local credentials")))
					(define (set-alist values key value) (let loop ((in values) (out '()))
									       (cond ((null? in) (reverse (cons (list key value) out))) ((eq? (caar in) key)
																	 (append (reverse out) (cons (list key value) (cdr in))))
										     (else (loop (cdr in) (cons (car in) out))))))
					(if (or (assoc 'meta arguments) (assoc 'metas arguments) (assoc 'meta? arguments))
					    (error 'argument-error "Metadata is not supported"))
					(if (and (eq? function 'pin!) (assoc 'response arguments))
					    (error 'argument-error "Pin proof responses are internal"))
                                        (if (and (eq? function 'resolve-batch)
                                                 (assoc 'proof? arguments)
                                                 (not invocation))
                                            (error 'argument-error
                                                   "Resolve batch proofs are internal"))
					(define (with-auth request) (cond (authentication
									   (append request `((authentication ,authentication))))
									  (invocation (append request `((invocation ,invocation))))
									  (else request)))
					(define (self-call data)
					  (set! delegated? #t)
					  (sync-call (with-auth
					              `((function ,function) (arguments ,arguments)
					                ,@(if data `((prepared ,data)) '()))) #t))
                                        (define (all? predicate values)
                                          (let loop ((values values))
                                            (if (null? values) #t
                                                (if (predicate (car values))
                                                    (loop (cdr values)) #f))))
                                        (define (pin-batch-proofs data paths)
                                          (if (not (and (list? data) (= (length data) 2)
                                                        (list? (car data)) (= (length (car data)) 2)
                                                        (eq? (caar data) 'proofs)
                                                        (list? (cadr data)) (= (length (cadr data)) 2)
                                                        (eq? (caadr data) 'slots)))
                                              (error 'argument-error "Malformed prepared pin batch"))
                                          (let ((proofs (cadar data)) (slots (cadadr data)))
                                            (if (not (and (list? proofs) (list? slots)
                                                          (= (length slots) (length paths))
                                                          (all? (lambda (proof)
                                                                    (and (list? proof) (= (length proof) 2)
                                                                         (equal? (map car proof) '(proof index))
                                                                         (list? (cadr (car proof)))
                                                                         (integer? (cadr (cadr proof)))))
                                                                  proofs)
                                                          (all? (lambda (slot)
                                                                    (or (not slot)
                                                                        (and (integer? slot) (>= slot 0)
                                                                             (< slot (length proofs)))))
                                                                  slots)))
                                                (error 'argument-error "Malformed prepared pin batch"))
                                            (let loop ((index 0))
                                              (if (< index (length proofs))
                                                  (if (not (member index slots))
                                                      (error 'argument-error
                                                             "Unreferenced prepared pin proof")
                                                      (loop (+ index 1)))))
                                            data))
					(define (failure result) (let ((entry (and (pair? result) (pair? (car result))
										   (assoc 'failure result))))
								   (if entry (let ((value (cadr entry))) (error (cadr (assoc 'tag value))
														(cadr (assoc 'message value))))
								       result)))
					(define (federation-call method operands) (let* ((input (if (not prepared) operands
												    (append (reverse (cdr (reverse operands)))
													    (list `((value ,(car (reverse operands)))
														    (continuation ,prepared))))))
                                                                                         (input
                                                                                          (append input
                                                                                                  (list
                                                                                                   (transient-key
                                                                                                    'sync-web/federation-continuation-signing-key/v1
                                                                                                    (request-secret)))))
											 (result (apply (federation method) (cons ledger input)))
											 (next (and (pair? result) (pair? (car result)) (assoc 'continuation result))))
										    (cond (next (failure (self-call (cadr next)))) (prepared result)
											  (else (failure result)))))
					(define (authenticate) (if invocation
								   (let ((source (arg-from invocation 'route-source))
									 (target (arg-from invocation 'route-target)))
								     (if (and (null? source) (null? target))
									 (let ((identity (arg-from invocation 'identity))) (if (not (equal?
																     (sync-hash (expression->byte-vector
																		 (arg-from invocation 'credentials)))
																     ((root 'get) '(interface secret))))
															       (error 'authentication-error "Authentication failed"))
									      (if (eq? identity '*journal*) '()
										  (if (eq? identity '*public*) '(*public*) `(*state* ,identity))))
									 (let* ((result ((federation 'authenticate) ledger
											 `((function ,function) (arguments ,arguments)
											   (invocation ,invocation)))))
									   (set! context (cadr (assoc 'context result)))
									   (set! auth-head
										 (and (assoc 'data-head result)
										      `((index ,(cadr (assoc 'data-head-index result)))
										        (object ,(cadr (assoc 'data-head result))))))
									   (cadr (assoc 'principal result)))))
								   (if (not authentication) '(*public*)
								       (let ((principal (or (arg-from authentication 'identity) '())))
									 (if (not (equal? (sync-hash (expression->byte-vector
												      (arg-from authentication 'credentials)))
											  ((root 'get) '(interface secret))))
									     (error 'authentication-error "Authentication failed"))
									 principal))))
					(define (arg-from values key) (let ((entry (and (list? values) (assoc key values))))
									(and entry (cadr entry))))
					(define (committed-path path)
					  (define (reject) (error 'path-error "Invalid committed path: ~S" path))
					  (if (not (and (list? path) (pair? path) (integer? (car path))
					                (pair? (cdr path)))) (reject))
					  (let ((origin-index (car path)) (tail (cdr path)))
					    (if (memq (car tail) '(*state* *transition* *crypto* *bridge*))
					        `((route ()) (history (,origin-index)) (path ,path))
					        (let loop ((segments tail) (route '()) (history (list origin-index)))
					          (if (not (and (pair? segments) (symbol? (car segments))
					                        (not (memq (car segments) '(*state* *transition* *crypto* *bridge*)))
					                        (pair? (cdr segments)) (integer? (cadr segments))
					                        (pair? (cddr segments))))
					              (reject))
					          (let ((alias (car segments)) (index (cadr segments))
					                (rest (cddr segments)))
					            (if (eq? (car rest) '*state*)
					                `((route ,(reverse (cons alias route)))
					                  (history ,(reverse (cons index history)))
					                  (path ,(cons index rest)))
					                (loop rest (cons alias route) (cons index history))))))))
					(define (origin-identity principal)
					  (cond ((equal? principal '(*public*)) '*public*)
					        ((null? principal) '*journal*)
					        ((and (pair? principal) (eq? (car principal) '*state*)
					              (pair? (cdr principal)) (symbol? (cadr principal))
					              (null? (cddr principal))) (cadr principal))
					        (else (error 'authentication-error "Invalid local federation identity"))))
                                        (define (authorize-path principal path operation)
                                          (let* ((admins ((root 'get) '(interface admins)))
                                                 (remote (assoc 'authentication-index context))
                                                 (direct?
                                                  (and (not remote)
                                                       (or (null? principal)
                                                           (member principal admins))))
                                                 (ctx `((latest-index ,(- ((ledger 'size)) 1))
                                                        ,@(if remote
                                                              `((authentication-index
                                                                 ,(cadr remote))) '()))))
                                            (if direct? 'direct
                                                (and path
                                                     ((authorization 'authorized?)
                                                      principal ctx path operation)))))
                                        (define (authorize-batch paths operation)
                                          (if (not (list? paths))
                                              (error 'argument-error
                                                     "Batch paths must be a proper list"))
                                          (if (> (length paths) 1024)
                                              (error 'argument-error
                                                     "Batch path count exceeds 1024"))
                                          (let ((principal (authenticate)))
                                            (let loop ((paths paths) (ancestors '()))
                                              (if (null? paths)
                                                  (list principal (reverse ancestors))
                                                  (let ((decision
                                                         (authorize-path principal
                                                                         (car paths)
                                                                         operation)))
                                                    (if (not decision)
                                                        (error 'authorization-error
                                                               "Not authorized"))
                                                    (loop (cdr paths)
                                                          (cons (eq? decision 'ancestor)
                                                                ancestors)))))))
                                        (define (authorize)
                                          (let* ((principal (authenticate))
                                                 (path
                                                  (let ((path (arg 'path)))
                                                    (if (and (eq? function 'trace)
                                                             (arg 'index)
                                                             (or (null? path)
                                                                 (not (integer? (car path)))))
                                                        (cons (arg 'index) path) path)))
                                                 (admins ((root 'get) '(interface admins)))
                                                 (remote (assoc 'authentication-index context))
                                                 (owner (arg 'user))
                                                 (conditional? (assoc 'expected arguments))
                                                 (periodic-write?
                                                  (and (memq function '(set! set-batch!))
                                                       (if (eq? function 'set!)
                                                           (equal? path '(*state* *periodic*))
                                                           (let loop ((paths (arg 'paths)))
                                                             (and (pair? paths)
                                                                  (or (equal? (car paths)
                                                                              '(*state* *periodic*))
                                                                      (loop (cdr paths))))))))
                                                 (direct?
                                                  (or (and (not remote)
                                                           (or (null? principal)
                                                               (member principal admins)))
                                                      (and (memq function
                                                                 '(authorizations authorize!
                                                                   deauthorize!))
                                                           owner
                                                           (equal? owner principal))))
                                                 (decision
                                                  (cond
                                                   (periodic-write? (and direct? 'direct))
                                                   ((eq? function 'call!)
                                                    (and direct? 'direct))
                                                   ((eq? function 'set-batch!)
                                                    (let ((writes
                                                           (authorize-batch
                                                            (arg 'paths) 'set!))
                                                          (reads
                                                           (and conditional?
                                                                (authorize-batch
                                                                 (arg 'paths) 'get))))
                                                      (and (not (member #t (cadr writes)))
                                                           (or (not reads)
                                                               (not (member #t
                                                                            (cadr reads))))
                                                           'direct)))
                                                   ((and (eq? function 'set!) conditional?)
                                                    (let ((write
                                                           (authorize-path principal path 'set!))
                                                          (read
                                                           (authorize-path principal path 'get)))
                                                      (and write read
                                                           (not (eq? write 'ancestor))
                                                           (not (eq? read 'ancestor))
                                                           'direct)))
                                                   (direct? 'direct)
                                                   (else
                                                    (authorize-path principal path function)))))
                                            (if (not decision)
                                                (error 'authorization-error "Not authorized"))
                                            (set! ancestor? (eq? decision 'ancestor))
                                            principal))
					(define (encode value expression?) (cond ((equal? value '(nothing)) value)
										 (expression? (expression->byte-vector value))
										 ((byte-vector? value) value)
										 (else (error 'value-error "Expected byte-vector"))))
					(define (decode value expression?) (cond ((not expression?) value)
										 ((byte-vector? value) (byte-vector->expression value))
										 ((and (pair? value) (pair? (car value)) (assoc 'content value))
										  (set-alist value 'content (decode (cadr (assoc 'content value)) #t)))
										 (else value)))
					(define* (stage-directory value (admitted? ancestor?)) (if (not admitted?) value
									    (let ((entries (and (pair? value) (eq? (car value) 'directory) (cadr value))))
									      (if (not entries) (error 'authorization-error "Not a directory"))
									      `(directory ,(let loop ((in entries) (out '())) (if (null? in) (reverse out)
																  (let* ((name (caar in)) (text (symbol->string name))) (loop (cdr in)
																							      (if (and (> (length text) 1) (char=? (text 0) #\*)
																								       (char=? (text (- (length text) 1)) #\*))
																								  out (cons (car in) out)))))) #f))))
					(define* (hydrate path (index -1))
					  ((federation 'route) ledger
					   `((operation hydrate) (path ,path) (index ,index))))
					(define (federate) (if (not (memq function '(get set! get-batch set-batch!)))
							       (error 'api-error "Not federated"))
					  (if (not (equal? (sync-hash
							    (expression->byte-vector (arg-from invocation 'credentials)))
							   ((root 'get) '(interface secret))))
					      (error 'authentication-error "Authentication failed"))
					  ((federation 'invoke) ledger function arguments (arg-from invocation 'route-target)
					   (arg-from invocation 'history-indexes)
					   (arg-from invocation 'identity)
                                           (transient-key 'sync-web/interface-signing-key/v1
                                                          (request-secret))))
					(define* (committed-invoke operation principal selected invoke-arguments
					                           (indexed-proof? #f))
					  ((federation 'invoke) ledger operation
					   (set-alist invoke-arguments 'path (arg-from selected 'path))
					   (arg-from selected 'route) (arg-from selected 'history)
					   (origin-identity principal)
                                           (transient-key 'sync-web/interface-signing-key/v1
                                                          (request-secret))
					   indexed-proof?))
                                        (define (committed-groups paths)
                                          (if (not (list? paths))
                                              (error 'argument-error
                                                     "Batch paths must be a proper list"))
                                          (if (> (length paths) 1024)
                                              (error 'argument-error
                                                     "Batch path count exceeds 1024"))
                                          (let loop ((paths paths) (index 0) (groups '()))
                                            (if (null? paths) groups
                                                (let* ((selected
                                                        (committed-path (car paths)))
                                                       (route
                                                        (arg-from selected 'route))
                                                       (history
                                                        (arg-from selected 'history))
                                                       (item
                                                        `((index ,index)
                                                          (path ,(car paths))
                                                          (selected ,selected))))
                                                  (let insert ((groups groups) (out '()))
                                                    (cond
                                                     ((null? groups)
                                                      (loop
                                                       (cdr paths) (+ index 1)
                                                       (append
                                                        (reverse out)
                                                        `(((route ,route)
                                                           (history ,history)
                                                           (items (,item)))))))
                                                     ((and
                                                       (equal?
                                                        (arg-from (car groups) 'route)
                                                        route)
                                                       (equal?
                                                        (arg-from (car groups) 'history)
                                                        history))
                                                      (let ((updated
                                                             (set-alist
                                                              (car groups) 'items
                                                              (append
                                                               (arg-from
                                                                (car groups) 'items)
                                                               (list item)))))
                                                        (loop
                                                         (cdr paths) (+ index 1)
                                                         (append
                                                          (reverse out)
                                                          (cons updated
                                                                (cdr groups))))))
                                                     (else
                                                      (insert (cdr groups)
                                                              (cons (car groups)
                                                                    out)))))))))
                                        (define (call-program program)
                                          (define (split-keywords values)
                                            (let loop ((remaining values) (positionals '())
                                                       (keywords '()))
                                              (cond
                                               ((null? remaining)
                                                (list (reverse positionals) (reverse keywords)))
                                               ((keyword? (car remaining))
                                                (if (null? (cdr remaining))
                                                    (error 'argument-error
                                                           "Missing value for keyword: ~S"
                                                           (car remaining)))
                                                (loop (cddr remaining) positionals
                                                      (cons (list
                                                             (keyword->symbol (car remaining))
                                                             (cadr remaining))
                                                            keywords)))
                                               (else
                                                (loop (cdr remaining)
                                                      (cons (car remaining) positionals)
                                                      keywords)))))
                                          (define (api-arguments method values)
                                            (let* ((split (split-keywords values))
                                                   (positionals (car split))
                                                   (keywords (cadr split)))
                                              (case method
                                                ((set!)
                                                 (let ((result
                                                        (cond
                                                         ((>= (length positionals) 2)
                                                          (append
                                                           `((path ,(car positionals))
                                                             (value ,(cadr positionals)))
                                                           keywords))
                                                         ((= (length positionals) 1)
                                                          (cons `(path ,(car positionals))
                                                                keywords))
                                                         ((assoc 'path keywords) keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "set! requires a path")))))
                                                   (if (or (assoc 'expression? result)
                                                           (not (assoc 'value result)))
                                                       result
                                                       (cons '(expression? #t) result))))
                                                ((get resolve)
                                                 (let ((result
                                                        (cond
                                                         ((pair? positionals)
                                                          (cons `(path ,(car positionals))
                                                                keywords))
                                                         ((assoc 'path keywords) keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "~S requires a path" method)))))
                                                   (if (assoc 'expression? result) result
                                                       (cons '(expression? #t) result))))
                                                ((get-batch resolve-batch pin-batch! unpin-batch!)
                                                 (let ((result
                                                        (cond
                                                         ((pair? positionals)
                                                          (cons `(paths ,(car positionals))
                                                                keywords))
                                                         ((assoc 'paths keywords) keywords)
                                                         (else
                                                          (error 'argument-error
                                                                 "~S requires paths" method)))))
                                                   (if (and (memq method
                                                                  '(get-batch resolve-batch))
                                                            (not (assoc 'expression? result)))
                                                       (cons '(expression? #t) result)
                                                       result)))
                                                ((call!)
                                                 (cond
                                                  ((>= (length positionals) 2)
                                                   (append
                                                    `((path ,(car positionals))
                                                      (arguments ,(cadr positionals)))
                                                    keywords))
                                                  ((and (assoc 'path keywords)
                                                        (assoc 'arguments keywords))
                                                   keywords)
                                                  (else
                                                   (error 'argument-error
                                                          "call! requires a path and arguments list"))))
                                                ((pin! unpin! delete-bridge!)
                                                 (if (null? positionals)
                                                     (error 'argument-error
                                                            "~S requires one argument" method))
                                                 (cons
                                                  (list
                                                   (if (eq? method 'delete-bridge!) 'name 'path)
                                                   (car positionals))
                                                  keywords))
                                                (else
                                                 (cond
                                                  ((null? positionals) keywords)
                                                  ((and (= (length positionals) 1)
                                                        (list? (car positionals)))
                                                   (append (car positionals) keywords))
                                                  (else
                                                   (error 'argument-error
                                                          "Use named arguments for ~S: ~S"
                                                          method values)))))))
                                          (define (safe-program-datum? value)
                                            (cond
                                             ((or (null? value) (boolean? value)
                                                  (number? value) (char? value)
                                                  (string? value) (symbol? value)
                                                  (keyword? value) (byte-vector? value)
                                                  (sync-node? value)) #t)
                                             ((syntax? value)
                                              (let loop
                                                  ((names
                                                    '(and begin case cond define define* do else if
                                                      lambda lambda* let let* letrec letrec* or
                                                      quasiquote quote set! unquote
                                                      unquote-splicing)))
                                                (and (pair? names)
                                                     (or (eq? value ((rootlet) (car names)))
                                                         (loop (cdr names))))))
                                             ((pair? value)
                                              (and (safe-program-datum? (car value))
                                                   (safe-program-datum? (cdr value))))
                                             ((vector? value)
                                              (let loop ((index 0))
                                                (or (= index (vector-length value))
                                                    (and (safe-program-datum?
                                                          (vector-ref value index))
                                                         (loop (+ index 1))))))
                                             (else #f)))
                                          (define (forbidden-program-symbol? value)
                                            (cond
                                             ((symbol? value)
                                              (memq value
                                                    '(*rootlet-redefinition-hook*
                                                      *read-error-hook* *error-hook*
                                                      *autoload-hook* *load-hook*
                                                      *missing-close-paren-hook*
                                                      *unbound-variable-hook* *s7*
                                                      let-set! let-ref unlet)))
                                             ((pair? value)
                                              (or (forbidden-program-symbol? (car value))
                                                  (forbidden-program-symbol? (cdr value))))
                                             ((vector? value)
                                              (let loop ((index 0))
                                                (and (< index (vector-length value))
                                                     (or (forbidden-program-symbol?
                                                          (vector-ref value index))
                                                         (loop (+ index 1))))))
                                             (else #f)))
                                          (define (safe-result? value)
                                            (cond ((or (null? value) (boolean? value)
                                                       (number? value) (char? value)
                                                       (string? value) (symbol? value)
                                                       (keyword? value) (byte-vector? value)
                                                       (sync-node? value)) #t)
                                                  ((pair? value)
                                                   (and (safe-result? (car value))
                                                        (safe-result? (cdr value))))
                                                  ((vector? value)
                                                   (let loop ((index 0))
                                                     (or (= index (vector-length value))
                                                         (and (safe-result?
                                                               (vector-ref value index))
                                                              (loop (+ index 1))))))
                                                  (else #f)))
                                          (let ((call-arguments (arg 'arguments)))
                                            (if (not (safe-program-datum? program))
                                                (error 'value-error
                                                       "call! program contains a captured evaluator object"))
                                            (if (forbidden-program-symbol? program)
                                                (error 'value-error
                                                       "call! program names a forbidden evaluator binding"))
                                            (if (not (proper-list? call-arguments))
                                                (error 'argument-error
                                                       "call! arguments must be a proper list"))
                                            (if (not (safe-program-datum? call-arguments))
                                                (error 'value-error
                                                       "call! arguments contain an evaluator capability"))
                                            (let* ((environment (inlet))
                                                   (root-environment (rootlet))
                                                   (capabilities
                                                    '(* + - / < <= = > >= and append apply
                                                      apply-values ash assq assoc begin boolean?
                                                      byte-vector? byte-vector->hex-string
                                                      byte-vector-length
                                                      byte-vector-ref byte-vector-set! caadar caadr
                                                      caar cadar cadr caddr car case catch cdadar
                                                      cddar cddr cdr char? complex? cond cons define
                                                      define* do else eq? equal? eqv? error even? expt
                                                      for-each if integer?
                                                      keyword? keyword->symbol lambda lambda* length
                                                      let let* letrec letrec* list list-tail list-values
                                                      list? logand macro? make-list map max member memq
                                                      min modulo negative? not null? number->string
                                                      number? odd? or pair? positive? procedure?
                                                      proper-list? quasiquote quote rational? real?
                                                      remainder reverse set! string->symbol string?
                                                      string=? string-length string-ref substring
                                                      subvector symbol->string symbol? throw unquote
                                                      unquote-splicing vector vector-length vector-ref
                                                      zero?)))
                                              (for-each
                                               (lambda (binding)
                                                 (catch #t
                                                   (lambda ()
                                                     (varlet environment
                                                             (car binding) '*removed*))
                                                   (lambda args #f)))
                                               root-environment)
                                              (for-each
                                               (lambda (name)
                                                 (varlet environment name
                                                         (root-environment name)))
                                               capabilities)
                                              (coverlet environment)
                                              (let* ((procedure (eval program environment))
                                                     (journal
                                                      (lambda (method)
                                                        (if (not (symbol? method))
                                                            (error 'argument-error
                                                                   "Journal method must be a symbol: ~S"
                                                                   method))
                                                        (lambda values
                                                          (let ((call-arguments
                                                                 (api-arguments method values)))
                                                            (if (not (safe-program-datum? call-arguments))
                                                                (error 'value-error
                                                                       "Journal arguments cannot expose capabilities"))
                                                            (set! delegated? #t)
                                                            (let ((result
                                                                   (sync-call
                                                                    (with-auth
                                                                     `((function ,method)
                                                                       (arguments
                                                                        ,call-arguments)))
                                                                    #t)))
                                                              (if (and (pair? result)
                                                                       (eq? (car result) 'error))
                                                                  (apply error (cdr result))
                                                                  result)))))))
                                                (if (not (procedure? procedure))
                                                    (error 'value-error
                                                           "call! path must contain a procedure"))
                                                (set! delegated? #t)
                                                (let ((result
                                                       (apply procedure
                                                              (cons journal call-arguments))))
                                                  (if (not (safe-result? result))
                                                      (error 'value-error
                                                             "call! result cannot expose capabilities")
                                                      result))))))
					(define (config-view path)
  (if (not (and (pair? path) (eq? (car path) 'private)))
      ((ledger 'config) path)
      (case (and (pair? (cdr path)) (cadr path))
        ((bridge bridge-retired)
         ((federation 'config)
          (cons (if (eq? (cadr path) 'bridge) 'peers 'retired)
                (cddr path))))
        ((bridge-identity)
         (let* ((alias (and (pair? (cddr path)) (caddr path)))
                (active ((federation 'config) `(peers ,alias identity-id))))
           (if (null? active)
               ((federation 'config) `(retired ,alias identity-id)) active)))
        ((bridge-preapproval)
         ((ledger 'config) (cons 'peer-preapproval (cddr path))))
        (else '()))))

					(if (and invocation (pair? (arg-from invocation 'route-target))) (federate)
					    (let ((result (case function ((route) ((federation 'route) ledger
										   `((route-source ,(or (arg 'route-source) '()))
										     (route-target ,(arg 'route-target))
										     (index ,(or (arg 'index) -1))
										     ,@(if (assoc 'history-indexes arguments)
											   `((history-indexes ,(arg 'history-indexes))) '())
										     ,@(if (assoc 'history-head-index arguments) `((history-head-index
																    ,(arg 'history-head-index))) '())
										     (roots ,(or (arg 'roots) '())))))
								((size) ((ledger 'size)))
								((info) ((ledger 'descriptor) -1))
								((synchronize!) ((federation 'synchronize!) ledger arguments))
								(else
                                                                 (if (memq function
                                                                           '(get-batch resolve-batch
                                                                             pin-batch! unpin-batch!
                                                                             trace-batch))
                                                                     (case function
                                                                       ((get-batch)
                                                                        (let* ((paths (arg 'paths))
                                                                               (auth
                                                                                (authorize-batch
                                                                                 paths 'get))
                                                                               (ancestors (cadr auth))
                                                                               (values
                                                                                ((ledger 'get-batch)
                                                                                 paths)))
                                                                          (let loop ((paths paths)
                                                                                     (values values)
                                                                                     (ancestors ancestors)
                                                                                     (results '()))
                                                                            (if (null? paths)
                                                                                `((results
                                                                                   ,(reverse results)))
                                                                                (loop
                                                                                 (cdr paths)
                                                                                 (cdr values)
                                                                                 (cdr ancestors)
                                                                                 (cons
                                                                                  `((path ,(car paths))
                                                                                    (content
                                                                                     ,(stage-directory
                                                                                       (decode
                                                                                        (car values)
                                                                                        (arg 'expression?))
                                                                                       (car ancestors))))
                                                                                  results))))))
                                                                       ((resolve-batch)
                                                                        (let* ((paths (arg 'paths))
                                                                               (groups
                                                                                (committed-groups paths))
                                                                               (principal
                                                                                (authenticate))
                                                                               (slots
                                                                                (make-vector
                                                                                 (length paths) #f))
                                                                               (internal-proof?
                                                                                (and invocation
                                                                                     (arg 'proof?)))
                                                                               (proof #f))
                                                                          ;; Reject every locally decidable denial before
                                                                          ;; any remote group can perform external work.
                                                                          (for-each
                                                                           (lambda (group)
                                                                             (if (null?
                                                                                  (arg-from group
                                                                                            'route))
                                                                                 (for-each
                                                                                  (lambda (item)
                                                                                    (if (not
                                                                                         (authorize-path
                                                                                          principal
                                                                                          (arg-from item
                                                                                                    'path)
                                                                                          'resolve))
                                                                                        (error
                                                                                         'authorization-error
                                                                                         "Not authorized")))
                                                                                  (arg-from group
                                                                                            'items))))
                                                                           groups)
                                                                          (for-each
                                                                           (lambda (group)
                                                                             (let* ((route
                                                                                     (arg-from group
                                                                                               'route))
                                                                                    (history
                                                                                     (arg-from group
                                                                                               'history))
                                                                                    (items
                                                                                     (arg-from group
                                                                                               'items))
                                                                                    (group-paths
                                                                                     (map
                                                                                      (lambda (item)
                                                                                        (arg-from item
                                                                                                  'path))
                                                                                      items))
                                                                                    (ancestors
                                                                                     (if (pair? route)
                                                                                         (map
                                                                                          (lambda (path)
                                                                                            #f)
                                                                                          group-paths)
                                                                                         (map
                                                                                          (lambda (path)
                                                                                            (let ((decision
                                                                                                   (authorize-path
                                                                                                    principal
                                                                                                    path
                                                                                                    'resolve)))
                                                                                              (if (not decision)
                                                                                                  (error
                                                                                                   'authorization-error
                                                                                                   "Not authorized"))
                                                                                              (eq? decision
                                                                                                   'ancestor)))
                                                                                          group-paths)))
                                                                                    (values
                                                                                     (if (pair? route)
                                                                                         ((federation
                                                                                           'invoke-batch)
                                                                                          ledger
                                                                                          (set-alist
                                                                                           arguments
                                                                                           'paths
                                                                                           (map
                                                                                            (lambda (item)
                                                                                              (arg-from
                                                                                               (arg-from item
                                                                                                         'selected)
                                                                                               'path))
                                                                                            items))
                                                                                          route history
                                                                                          (origin-identity
                                                                                           principal)
                                                                                          (transient-key
                                                                                           'sync-web/interface-signing-key/v1
                                                                                           (request-secret)))
                                                                                         ((ledger
                                                                                           'resolve-batch)
                                                                                          group-paths #f
                                                                                          (and auth-head
                                                                                               (map
                                                                                                (lambda (path)
                                                                                                  auth-head)
                                                                                                group-paths))
                                                                                          ancestors
                                                                                          internal-proof?)))
                                                                                    (group-proof
                                                                                     (and internal-proof?
                                                                                          (cadr
                                                                                           (assoc 'proof
                                                                                                  values))))
                                                                                    (values
                                                                                     (if group-proof
                                                                                         (cadr
                                                                                          (assoc 'results
                                                                                                 values))
                                                                                         values)))
                                                                               (if group-proof
                                                                                   (if proof
                                                                                       (error 'integrity-error
                                                                                              "Terminal batch produced multiple proofs")
                                                                                       (set! proof group-proof)))
                                                                               (let loop ((items items)
                                                                                          (values values))
                                                                                 (if (pair? items)
                                                                                     (begin
                                                                                       (vector-set!
                                                                                        slots
                                                                                        (arg-from
                                                                                         (car items)
                                                                                         'index)
                                                                                        (car values))
                                                                                       (loop
                                                                                        (cdr items)
                                                                                        (cdr values)))))))
                                                                           groups)
                                                                          (let loop ((paths paths)
                                                                                     (index 0)
                                                                                     (results '()))
                                                                            (if (null? paths)
                                                                                `((results
                                                                                   ,(reverse results))
                                                                                  ,@(if proof
                                                                                        `((proof ,proof))
                                                                                        '()))
                                                                                (let ((content
                                                                                       (decode
                                                                                        (cadr
                                                                                         (assoc
                                                                                          'content
                                                                                          (vector-ref
                                                                                           slots index)))
                                                                                        (arg
                                                                                         'expression?))))
                                                                                  (loop
                                                                                   (cdr paths)
                                                                                   (+ index 1)
                                                                                   (cons
                                                                                    `((path ,(car paths))
                                                                                      (content ,content)
                                                                                      ,@(if (arg 'pinned?)
                                                                                            `((pinned?
                                                                                               ,((ledger
                                                                                                  'pinned?)
                                                                                                 (car paths))))
                                                                                            '()))
                                                                                    results)))))))
                                                                       ((pin-batch!)
                                                                        (let ((paths (arg 'paths)))
                                                                          (authorize-batch paths 'pin!)
                                                                          (if prepared
                                                                              ((ledger 'pin-batch!)
                                                                               paths
                                                                               (pin-batch-proofs prepared paths))
                                                                              (let* ((groups
                                                                                      (committed-groups
                                                                                       paths))
                                                                                     (principal
                                                                                      (authenticate))
                                                                                     (proof-slots
                                                                                      (make-vector
                                                                                       (length paths)
                                                                                       #f))
                                                                                     (proof-groups '())
                                                                                     (remote? #f))
                                                                                ;; Resolve every remote proof group in
                                                                                ;; this read-only phase. A blocking
                                                                                ;; self-call performs the later atomic
                                                                                ;; retention mutation.
                                                                                (for-each
                                                                                 (lambda (group)
                                                                                   (let* ((route
                                                                                           (arg-from
                                                                                            group 'route))
                                                                                          (history
                                                                                           (arg-from
                                                                                            group 'history))
                                                                                          (items
                                                                                           (arg-from
                                                                                            group 'items)))
                                                                                     (if (pair? route)
                                                                                         (let* ((_ (set! remote?
                                                                                                         #t))
                                                                                                (resolved
                                                                                                 ((federation
                                                                                                   'invoke-batch)
                                                                                                  ledger
                                                                                                  (set-alist
                                                                                                   arguments
                                                                                                   'paths
                                                                                                   (map
                                                                                                    (lambda
                                                                                                     (item)
                                                                                                      (arg-from
                                                                                                       (arg-from
                                                                                                        item
                                                                                                        'selected)
                                                                                                       'path))
                                                                                                    items))
                                                                                                  route history
                                                                                                  (origin-identity
                                                                                                   principal)
                                                                                                  (transient-key
                                                                                                   'sync-web/interface-signing-key/v1
                                                                                                   (request-secret))
                                                                                                  #t))
                                                                                                (proof
                                                                                                 `((proof
                                                                                                    ,(cadr
                                                                                                      (assoc
                                                                                                       'proof
                                                                                                       resolved)))
                                                                                                   (index
                                                                                                    ,(cadr
                                                                                                      (assoc
                                                                                                       'proof-index
                                                                                                       resolved)))))
                                                                                                (slot
                                                                                                 (length
                                                                                                  proof-groups)))
                                                                                           (set! proof-groups
                                                                                                 (append proof-groups
                                                                                                         (list proof)))
                                                                                           (for-each
                                                                                            (lambda (item)
                                                                                              (vector-set!
                                                                                               proof-slots
                                                                                               (arg-from
                                                                                                item 'index)
                                                                                               slot))
                                                                                            items)))))
                                                                                 groups)
                                                                                (let ((slots
                                                                                       (let loop
                                                                                           ((index 0)
                                                                                            (out '()))
                                                                                         (if (= index
                                                                                                (length paths))
                                                                                             (reverse out)
                                                                                             (loop
                                                                                              (+ index 1)
                                                                                              (cons
                                                                                               (vector-ref
                                                                                                proof-slots
                                                                                                index)
                                                                                               out))))))
                                                                                  (if remote?
                                                                                      (failure
                                                                                       (self-call
                                                                                        `((proofs ,proof-groups)
                                                                                          (slots ,slots))))
                                                                                      ((ledger
                                                                                        'pin-batch!)
                                                                                       paths
                                                                                       slots)))))))
                                                                       ((unpin-batch!)
                                                                        (let ((paths (arg 'paths)))
                                                                          (authorize-batch paths 'unpin!)
                                                                          ((ledger 'unpin-batch!) paths)))
                                                                       ((trace-batch)
                                                                        (let* ((index (or (arg 'index) -1))
                                                                               (paths
                                                                                (map
                                                                                 (lambda (path)
                                                                                   (if (and (pair? path)
                                                                                            (integer?
                                                                                             (car path)))
                                                                                       path
                                                                                       (cons index path)))
                                                                                 (arg 'paths)))
                                                                               (auth
                                                                                (authorize-batch
                                                                                 paths 'trace))
                                                                               (ancestors (cadr auth)))
                                                                          (if (member #t ancestors)
                                                                              (cadr
                                                                               (assoc
                                                                                'proof
                                                                                ((ledger
                                                                                  'resolve-batch)
                                                                                 paths #f
                                                                                 (and (arg 'head)
                                                                                      (map
                                                                                       (lambda (path)
                                                                                         (arg 'head))
                                                                                       paths))
                                                                                 ancestors #t)))
                                                                              ((ledger 'trace-batch)
                                                                               paths (arg 'head)))))
                                                                       (else
                                                                        (error 'api-error
                                                                               "Unknown batch function")))
								 (let ((selected (and (memq function '(resolve pin! unpin!))
								                      (committed-path (arg 'path)))))
								   (cond
								    ((and (eq? function 'resolve) (pair? (arg-from selected 'route)))
								     (committed-invoke 'resolve (authenticate) selected arguments))
								    ((and (eq? function 'pin!) (pair? (arg-from selected 'route)))
								     (let ((principal (authorize)))
								       (if prepared ((ledger 'pin!) (arg 'path) prepared)
								           (let* ((resolve-arguments
								                   (set-alist (set-alist arguments 'proof? #t) 'pinned? #f))
								                  (resolved (committed-invoke 'resolve principal selected
								                                              resolve-arguments #t))
								                  (proof (and (list? resolved) (assoc 'proof resolved)
								                              (cadr (assoc 'proof resolved))))
								                  (proof-index
								                   (and (list? resolved) (assoc 'proof-index resolved)
								                        (cadr (assoc 'proof-index resolved)))))
								             (if (not (and proof (integer? proof-index)))
								                 (error 'integrity-error "Federated resolve did not return indexed proof"))
								             (failure
								              (self-call `((proof ,proof) (index ,proof-index))))))))
								    ((and (eq? function 'unpin!) (pair? (arg-from selected 'route)))
								     (authorize)
								     ((ledger 'unpin!) (arg 'path)))
								    (else
                                                                     (authorize)
								     (case function ((get) (stage-directory
													 (decode ((ledger 'get) (arg 'path)) (arg 'expression?))))
											((set!)
                                                                                         (let ((expected
                                                                                                (assoc 'expected arguments)))
                                                                                           ((ledger 'set!)
                                                                                            (arg 'path)
                                                                                            (encode (arg 'value)
                                                                                                    (arg 'expression?))
                                                                                            (if expected #t #f)
                                                                                            (and expected
                                                                                                 (encode
                                                                                                  (cadr expected)
                                                                                                  (arg 'expression?))))))
											((set-batch!)
                                                                                         (let* ((paths (arg 'paths))
                                                                                                (values (arg 'values))
                                                                                                (expected-entry
                                                                                                 (assoc 'expected arguments))
                                                                                                (expected
                                                                                                 (and expected-entry
                                                                                                      (cadr expected-entry))))
                                                                                           (if (not
                                                                                                (and (list? paths)
                                                                                                     (list? values)
                                                                                                     (= (length paths)
                                                                                                        (length values))
                                                                                                     (or (not expected-entry)
                                                                                                         (and (list? expected)
                                                                                                              (= (length paths)
                                                                                                                 (length expected))))))
                                                                                               (error
                                                                                                'argument-error
                                                                                                "Batch paths, values, and expected values must have equal lengths"))
                                                                                           ((ledger 'set-batch!)
                                                                                            (if expected-entry
                                                                                                (map
                                                                                                 (lambda (path value old)
                                                                                                   (list
                                                                                                    path
                                                                                                    (encode value
                                                                                                            (arg 'expression?))
                                                                                                    (encode old
                                                                                                            (arg 'expression?))))
                                                                                                 paths values expected)
                                                                                                (map
                                                                                                 (lambda (path value)
                                                                                                   (list
                                                                                                    path
                                                                                                    (encode value
                                                                                                            (arg 'expression?))))
                                                                                                 paths values)))))
                                                                                         ((call!)
                                                                                          (call-program
                                                                                           (decode ((ledger 'get)
                                                                                                    (arg 'path)) #t)))
											((resolve) (let* ((path (arg 'path)) (head (or (arg 'head) auth-head))
													  (attempt ((ledger 'resolve) path #f #f head ancestor?))
													  (head (if (and (not head) (equal? attempt '(unknown))
															 (pair? path)
															 (let ((segment (if (integer? (car path))
																	    (cadr path) (car path))))
															   (or (eq? segment '*bridge*)
															       (and (symbol? segment) (not (memq segment
																				 '(*state* *transition* *crypto*)))))))
														    (hydrate path) head)))
												     (decode ((ledger 'resolve) path (arg 'pinned?) (arg 'proof?)
													      head ancestor?)
													     (arg 'expression?))))
											((trace) (let* ((path (arg 'path)) (index (or (arg 'index) -1))
													(trace-path (cons index path))
													(head (arg 'head)))
												   (if head ((ledger 'trace) path head)
												       (let* ((resolve-path (if (and (pair? path) (integer? (car path)))
													 path (cons index path)))
											  (attempt ((ledger 'resolve) resolve-path)))
													 (if (equal? attempt '(unknown)) ((ledger 'trace) path
																	  (hydrate path index))
													     ((ledger 'trace) trace-path))))))
											((pin!)
											 (if prepared ((ledger 'pin!) (arg 'path) prepared)
											     (let* ((path (arg 'path))
											            (attempt ((ledger 'resolve) path)))
											       (if (not (equal? attempt '(unknown)))
											           ((ledger 'pin!) path #f)
											           (failure
											            (self-call
											             `((proof ,((ledger 'trace) path (hydrate path)))
											               (index ,(- ((ledger 'size)) 1)))))))))
                                          ((unpin!) ((ledger 'unpin!) (arg 'path)))
											((bridge!) (federation-call 'bridge! (list (arg 'name)
																   `((interface ,(arg 'interface))
																     (remote-name ,(arg 'remote-name))))))
											((delete-bridge!) ((authorization 'deauthorize!)
													   `((principal-prefix (,(arg 'name)))))
											 ((federation 'delete-bridge!) ledger (arg 'name)))
											((authorizations) ((authorization 'authorizations) (arg 'user)))
											((authorize!) ((authorization 'authorize!)
												       `((user ,(arg 'user)) (rule ,(arg 'rule))
													 (latest-index ,(- ((ledger 'size)) 1)))))
											((deauthorize!) ((authorization 'deauthorize!)
													 `((user ,(arg 'user)) (rule ,(arg 'rule)))))
											((config) (config-view (or (arg 'path) '())))
											((update-config!) ((ledger 'update-config!)
													   `((path ,(arg 'path)) (value ,(arg 'value))))
											 (if (equal? (arg 'path) '(public bridge-accept))
											     ((federation 'update-config!) `((path (bridge-accept))
															     (value ,(arg 'value))))) #t)
											((*secret*)
                         (let* ((secret (arg 'secret))
                                (verifier (sync-hash (expression->byte-vector secret)))
                                (public-key
                                 (car (transient-key
                                       'sync-web/interface-signing-key/v1 secret))))
                           (if (equal? verifier ((root 'get) '(interface root-verifier)))
                               (error 'argument-error "Root and Interface credentials must differ"))
                           ((root 'set!) '(interface credential) secret)
                           ((root 'set!) '(interface secret) verifier)
                           ((federation 'update-config!)
                            `((path (public-key)) (value ,public-key)))
                           ((ledger 'update-config!)
                            `((path (public interface public-key))
                              (value ,public-key)))))
                                          ((*admins-get*) ((root 'get) '(interface admins)))
											((*admins-set*) (if (not (let loop ((in (arg 'admins))) (or (null? in)
																		    (and (pair? (car in)) (eq? (caar in) '*state*)
																			 (pair? (cdar in))
																			 (symbol? (cadar in))
																			 (null? (cddar in))
																			 (loop (cdr in))))))
													    (error 'argument-error "Admins must be local"))
											 ((root 'set!) '(interface admins) (arg 'admins)))
											((*window-set*) ((ledger 'update-config!)
													 `((path (public window)) (value ,(arg 'value)))))
											(else (error 'api-error "Unknown Interface function")))))))))))
					      (if (not delegated?)
					          (begin
					            ((root 'set!) '(root object ledger) (ledger))
					            ((root 'set!) '(root object federation) (federation))
					            ((root 'set!) '(root object authorization) (authorization))))
					      result)))))

  (set-query `(lambda (root query)
               (let ((once ,query-once)) (once query))))

  (define step-once '(lambda (root secret query)
		       (let* ((module (eval ((root 'get) '(root class standard-module))))
                              (std-node ((root 'get) '(root object standard)))
                              (standard (module 'local ((root 'get) '(root class standard)) std-node))
                              (ledger (module 'local ((root 'get) '(root class ledger))
                                              ((root 'get) '(root object ledger))))
                              (federation (module 'local ((root 'get) '(root class federation))
                                                  ((root 'get) '(root object federation))))
                              (identity ((ledger 'config) '(public identity)))
                              (id (cadr (assoc 'id identity)))
                              (continuation-key
                               (crypto-generate
                                (expression->byte-vector
                                 (list 'sync-web/federation-continuation-signing-key/v1 id
                                       (sync-hash (expression->byte-vector secret)))))))
                         (define (self query blocking?) (sync-call `(*step* ,secret ,query) blocking?))
			 (define (run method operands data query) (let* ((input (if data
										    (append (reverse (cdr (reverse operands)))
											    (list `((value ,(car (reverse operands))) (continuation ,data))))
										    operands))
                                                                         (input (append input (list continuation-key)))
									 (result (apply (federation method) (cons ledger input)))
									 (next (and (pair? result) (pair? (car result)) (assoc 'continuation result))))
								    (if next (self (append query (list (cadr next))) #t) result)))
                         (define (commit report?)
                           (let* ((before ((ledger 'size)))
                                  (keys (crypto-generate (expression->byte-vector
                                                         (list 'sync-web/journal-signing-key/v1 id
                                                               (sync-hash (expression->byte-vector secret))))))
                                  (size ((ledger 'step!) `((unix-time ,(system-time-unix))
                                                           (public-key ,(car keys))
                                                           (secret-key ,(cdr keys))))))
                             (if report?
                                 (list size (> size before)
                                       (not (equal? ((ledger 'get) '(*state* *periodic*))
                                                    '(nothing))))
                                 size)))
                         (let* ((query (if (null? query) '(ledger-step) query))
                                (outer-step? (and (eq? (car query) 'ledger-step)
                                                  (null? (cdr query))))
                                (result (case (car query)
											  ((ledger-step) (if (and (pair? (cdr query)) (cadr query))
                                                                                     (commit (and (pair? (cddr query)) (caddr query)))
													     (let ((ops (cadr (assoc 'operations ((federation 'step!) ledger)))))
													       (for-each (lambda (op) (self op #f)) ops)
                                                                               (let* ((commit-result (self '(ledger-step #t #t) #t))
                                                                                      (size (car commit-result)))
                                                                                 (if (and (cadr commit-result)
                                                                                          (caddr commit-result))
                                                                                     (sync-call
                                                                                      `((function call!)
                                                                                        (arguments
                                                                                         ((path (*state* *periodic*))
                                                                                          (arguments (,(- size 1)))))
                                                                                        (authentication
                                                                                         ((credentials
                                                                                           ,((root 'get) '(interface credential))))))
                                                                                      #f))
                                                                                 size))))
											  ((bridge-synchronize!) (run 'bridge-synchronize! (list (cadr query))
														      (and (pair? (cddr query)) (caddr query))
														      `(bridge-synchronize! ,(cadr query))))
											  (else (error 'api-error "Unknown step operation")))))
                           (if (not outer-step?)
                               (begin
                                 ((root 'set!) '(root object ledger) (ledger))
                                 ((root 'set!) '(root object federation) (federation))))
			   result))))
  (set-step step-once)
  "Installed interface")
